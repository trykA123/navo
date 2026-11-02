use std::time::{Duration, Instant};

use serde_json::Value;

use super::{
    PerfDocument, Recording, SystemFacts, THREAD_CAP, ThreadCpu, ThreadTime, perf_json,
    threads_text,
};
use crate::{Identity, Redactor, Salt};

fn redactor() -> Redactor {
    Redactor::new(
        Salt::from_bytes([7; 16]),
        Identity {
            user: Some("canary-user-7731".to_owned()),
            computer: None,
            domain: None,
            profile: None,
        },
    )
}

fn thread(id: u32, name: &str, cpu_ms: u64) -> ThreadTime {
    ThreadTime {
        id,
        name: (!name.is_empty()).then(|| name.to_owned()),
        cpu: Duration::from_millis(cpu_ms),
    }
}

fn track(steps: &[&[ThreadTime]]) -> ThreadCpu {
    let start = Instant::now();
    let mut cpu = ThreadCpu::default();
    for (second, threads) in (0u64..).zip(steps) {
        cpu.record(start + Duration::from_secs(second), threads);
    }
    cpu
}

fn json(cpu: &ThreadCpu) -> Value {
    let recording = Recording::new(Instant::now());
    let document = PerfDocument::new(
        &recording,
        Instant::now(),
        &SystemFacts::default(),
        &redactor(),
    )
    .with_threads(&cpu.report(), &redactor());
    serde_json::from_slice(&perf_json(&document).unwrap()).unwrap()
}

#[test]
fn two_samples_give_percent_of_one_core_for_the_interval() {
    let cpu = track(&[
        &[thread(7, "navo-io", 1_000), thread(8, "navo-job", 0)],
        &[thread(7, "navo-io", 1_500), thread(8, "navo-job", 1_000)],
    ]);
    let report = cpu.report();
    let io = &report.threads[&7];
    assert_eq!(io.total_cpu_ms, 500.0);
    assert_eq!(io.max_percent_of_one_core, 50.0);
    assert_eq!(report.threads[&8].p50_percent_of_one_core, 100.0);
}

#[test]
fn cpu_before_the_first_sample_is_not_counted() {
    let cpu = track(&[&[thread(7, "a", 90_000)]]);
    let report = cpu.report();
    assert_eq!(report.threads[&7].total_cpu_ms, 0.0);
    assert_eq!(report.threads[&7].max_percent_of_one_core, 0.0);
}

#[test]
fn a_thread_that_disappears_keeps_its_totals_and_stops_adding_intervals() {
    let cpu = track(&[
        &[thread(7, "gone", 0), thread(8, "stays", 0)],
        &[thread(7, "gone", 400), thread(8, "stays", 100)],
        &[thread(8, "stays", 200)],
        &[thread(8, "stays", 300)],
    ]);
    let report = cpu.report();
    assert_eq!(report.threads[&7].total_cpu_ms, 400.0);
    assert_eq!(report.threads[&7].intervals, 1);
    assert_eq!(report.threads[&8].intervals, 3);
}

#[test]
fn a_new_thread_counts_all_its_cpu_in_its_first_interval() {
    let cpu = track(&[
        &[thread(7, "old", 0)],
        &[thread(7, "old", 0), thread(9, "new", 250)],
        &[thread(7, "old", 0), thread(9, "new", 500)],
    ]);
    let report = cpu.report();
    assert_eq!(report.threads[&9].total_cpu_ms, 500.0);
    assert_eq!(report.threads[&9].max_percent_of_one_core, 25.0);
}

#[test]
fn a_reused_id_after_a_gap_starts_from_zero() {
    let cpu = track(&[
        &[thread(7, "first", 5_000)],
        &[],
        &[thread(7, "second", 300)],
    ]);
    let report = cpu.report();
    assert_eq!(report.threads[&7].total_cpu_ms, 300.0);
    assert_eq!(report.threads[&7].name, "second");
}

#[test]
fn a_counter_that_goes_backwards_never_underflows() {
    let cpu = track(&[&[thread(7, "a", 900)], &[thread(7, "a", 100)]]);
    assert_eq!(cpu.report().threads[&7].total_cpu_ms, 100.0);
}

#[test]
fn unnamed_threads_fall_back_to_their_id() {
    let cpu = track(&[&[thread(42, "", 0)], &[thread(42, "", 10)]]);
    assert_eq!(cpu.report().threads[&42].name, "thread 42");
}

#[test]
fn threads_past_the_cap_are_ignored_and_counted() {
    let over = 5;
    let many: Vec<ThreadTime> = (0..u32::try_from(THREAD_CAP + over).unwrap())
        .map(|id| thread(id, "w", 0))
        .collect();
    let cpu = track(&[&many, &many]);
    let report = cpu.report();
    assert_eq!(report.threads.len(), THREAD_CAP);
    assert_eq!(report.ignored_threads_max, over);
}

#[test]
fn the_top_ten_are_ordered_by_total_cpu_then_id() {
    let first: Vec<ThreadTime> = (1..=12).map(|id| thread(id, "w", 0)).collect();
    let second: Vec<ThreadTime> = (1..=12)
        .map(|id| thread(id, "w", u64::from(id.min(11)) * 10))
        .collect();
    let report = track(&[&first, &second]).report();
    let top: Vec<u32> = report.top().iter().map(|thread| thread.id).collect();
    assert_eq!(top, [11, 12, 10, 9, 8, 7, 6, 5, 4, 3]);
}

#[test]
fn the_document_carries_a_threads_section_and_the_top_threads() {
    let cpu = track(&[
        &[
            thread(7, "navo-io", 0),
            thread(8, "canary-user-7731 worker", 0),
        ],
        &[
            thread(7, "navo-io", 800),
            thread(8, "canary-user-7731 worker", 10),
        ],
    ]);
    let json = json(&cpu);
    let io = &json["threads"]["by_id"]["7"];
    assert_eq!(io["name"], "navo-io");
    assert_eq!(io["total_cpu_ms"], 800.0);
    assert_eq!(io["p50_percent_of_one_core"], 80.0);
    assert_eq!(io["max_percent_of_one_core"], 80.0);
    assert_eq!(json["threads"]["cap"], THREAD_CAP);
    assert_eq!(json["threads"]["ignored_threads_max"], 0);
    assert_eq!(json["summary"]["top_threads"][0]["id"], 7);
    assert_eq!(json["summary"]["top_threads"][1]["id"], 8);
    assert!(!json.to_string().contains("canary"));
}

#[test]
fn a_document_without_thread_samples_has_an_empty_section() {
    let json = json(&ThreadCpu::default());
    assert_eq!(
        json["threads"]["by_id"],
        Value::Object(serde_json::Map::new())
    );
    assert_eq!(json["summary"]["top_threads"], Value::Array(Vec::new()));
}

#[test]
fn the_text_lists_each_top_thread_with_total_and_peak() {
    let cpu = track(&[&[thread(7, "navo-io", 0)], &[thread(7, "navo-io", 1_234)]]);
    let text = threads_text(&cpu.report(), &redactor());
    assert!(
        text.contains("navo-io (7): 1234 ms total, peak 123.4 %"),
        "{text}"
    );
}
