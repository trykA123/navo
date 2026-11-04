use serde_json::Value;

use super::fake::{Fake, Metrics, SECRET};
use super::host::MeasureError;
use super::measure_start_and_idle;
use crate::fixture;
use crate::harness::phase::{Observer, Verdict};
use crate::privacy::{LeakScanner, Output, Report, SafeText, SafeWriter, assert_no_leaks};
use crate::probe::ProbeError;

#[derive(Default)]
struct Names {
    started: Vec<String>,
    verdicts: Vec<Verdict>,
}

impl Observer for Names {
    fn started(&mut self, name: SafeText) {
        self.started
            .push(serde_json::to_string(&name).unwrap_or_default());
    }
    fn finished(&mut self, _name: SafeText, verdict: Verdict) {
        self.verdicts.push(verdict);
    }
}

fn run(fake: &mut Fake) -> (Vec<Value>, Names) {
    let mut names = Names::default();
    let reports = measure_start_and_idle(fake, &mut names);
    let values = reports
        .iter()
        .map(|report| serde_json::to_value(report).unwrap())
        .collect();
    (values, names)
}

fn find<'a>(reports: &'a [Value], measure: &str) -> &'a Value {
    reports
        .iter()
        .find(|report| report["measure"] == measure)
        .unwrap()
}

fn start<'a>(reports: &'a [Value], kind: &str) -> &'a Value {
    reports
        .iter()
        .find(|report| report["measure"] == "start" && report["kind"] == kind)
        .unwrap()
}

fn reason(metric: &Value) -> &str {
    assert_eq!(metric["state"], "not-measured");
    metric["reason"].as_str().unwrap()
}

#[test]
fn phases_run_in_the_documented_order() {
    let mut fake = Fake::new();
    let (reports, names) = run(&mut fake);
    assert_eq!(
        names.started,
        [
            "\"prepare fixtures\"",
            "\"cold start\"",
            "\"warm start\"",
            "\"idle\"",
            "\"exe size\""
        ]
    );
    assert!(names.verdicts.iter().all(|v| matches!(v, Verdict::Ok)));
    assert_eq!(reports.len(), 4);
    let events = fake.events();
    assert_eq!(events.first().map(String::as_str), Some("prepare"));
    let launches: Vec<_> = events.iter().filter(|e| e.starts_with("launch")).collect();
    assert_eq!(launches.len(), 3 + 21 + 1);
}

#[test]
fn cold_start_uses_a_fresh_copy_for_each_of_three_runs() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake);
    let events = fake.events();
    for run in 1..=3 {
        let copy = events.iter().position(|e| *e == format!("copy {run}"));
        let launch = events
            .iter()
            .position(|e| *e == format!("launch {run} copy{run} metrics=true"));
        assert!(copy < launch && copy.is_some(), "run {run}");
    }
    let cold = start(&reports, "cold");
    assert_eq!(cold["attempted"], 3);
    assert_eq!(cold["first_usable"]["value"]["samples"], 3);
    assert_eq!(cold["first_usable"]["value"]["p95_is_max"], true);
    assert_eq!(cold["first_usable"]["value"]["median_micros"], 202_000);
}

#[test]
fn warm_start_drops_one_warm_up_and_summarises_twenty_runs() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake);
    let events = fake.events();
    let warm_launches = events
        .iter()
        .filter(|e| e.contains("original metrics=true"))
        .count();
    assert_eq!(warm_launches, 21);
    let warm = start(&reports, "warm");
    assert_eq!(warm["attempted"], 20);
    assert_eq!(warm["failed"], 0);
    let usable = &warm["first_usable"]["value"];
    assert_eq!(usable["samples"], 20);
    assert_eq!(usable["p95_is_max"], false);
    // Counted runs are 5..=24: the warm-up (run 4) is discarded.
    assert_eq!(usable["median_micros"], 214_500);
    assert_eq!(usable["p95_micros"], 223_000);
    assert_eq!(warm["first_frame"]["value"]["median_micros"], 114_500);
    assert_eq!(warm["first_window"]["value"]["median_micros"], 114_500);
}

#[test]
fn one_failed_run_is_recorded_and_the_rest_go_on() {
    let mut fake = Fake::new();
    fake.fail_launch = vec![8];
    let (reports, names) = run(&mut fake);
    let warm = start(&reports, "warm");
    assert_eq!(warm["attempted"], 20);
    assert_eq!(warm["failed"], 1);
    assert_eq!(warm["first_usable"]["value"]["samples"], 19);
    assert_eq!(warm["first_usable"]["value"]["p95_is_max"], true);
    assert_eq!(reports.len(), 4);
    assert!(names.verdicts.iter().all(|v| matches!(v, Verdict::Ok)));
}

#[test]
fn a_failed_cold_copy_counts_as_a_failed_run() {
    let mut fake = Fake::new();
    fake.fail_copy = vec![2];
    let (reports, _) = run(&mut fake);
    let cold = start(&reports, "cold");
    assert_eq!(
        (cold["attempted"].as_u64(), cold["failed"].as_u64()),
        (Some(3), Some(1))
    );
    assert_eq!(cold["first_window"]["value"]["samples"], 2);
}

#[test]
fn when_every_run_fails_the_phase_fails_with_a_reason() {
    let mut fake = Fake::new();
    fake.fail_launch = (1..=30).collect();
    let (reports, names) = run(&mut fake);
    let cold = start(&reports, "cold");
    assert_eq!(reason(&cold["first_usable"]), "process exited early");
    assert_eq!(cold["failed"], 3);
    let code = MeasureError::Probe(ProbeError::Exited).code();
    assert!(matches!(names.verdicts[1], Verdict::Failed(c) if c == code));
    assert!(matches!(names.verdicts[2], Verdict::Failed(c) if c == code));
    assert!(matches!(names.verdicts[4], Verdict::Ok));
}

#[test]
fn a_build_without_a_metrics_file_still_gives_window_time() {
    let mut fake = Fake::new();
    fake.metrics = Metrics::Absent;
    let (reports, _) = run(&mut fake);
    let cold = start(&reports, "cold");
    assert_eq!(cold["first_window"]["state"], "measured");
    assert_eq!(reason(&cold["first_frame"]), "build wrote no metrics file");
    assert_eq!(reason(&cold["first_usable"]), "build wrote no metrics file");
}

#[test]
fn a_metrics_file_without_start_numbers_is_not_measured() {
    let mut fake = Fake::new();
    fake.metrics = Metrics::Empty;
    let (reports, _) = run(&mut fake);
    let warm = start(&reports, "warm");
    assert_eq!(reason(&warm["first_usable"]), "metrics file unreadable");
}

#[test]
fn a_half_written_metrics_file_is_read_again() {
    let mut fake = Fake::new();
    fake.metrics = Metrics::HalfWritten(3);
    let (reports, _) = run(&mut fake);
    let cold = start(&reports, "cold");
    assert_eq!(cold["first_usable"]["state"], "measured");
}

#[test]
fn idle_reads_memory_at_30_s_cpu_and_wakeups_over_60_s_then_memory_at_the_end() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake);
    let events = fake.events();
    let idle_start = events
        .iter()
        .position(|e| e == "launch 25 original metrics=false");
    assert!(idle_start.is_some(), "idle is the last launch");
    let pauses: Vec<_> = events[idle_start.unwrap()..]
        .iter()
        .filter(|e| e.starts_with("pause"))
        .map(String::as_str)
        .collect();
    assert_eq!(pauses, ["pause 30", "pause 60", "pause 30"]);
    let idle = find(&reports, "idle");
    assert_eq!(
        idle["memory_at_settle"]["value"]["private_bytes"],
        50_000_000
    );
    assert_eq!(idle["memory_at_end"]["value"]["private_bytes"], 50_000_001);
    assert_eq!(idle["cpu"]["value"]["window_micros"], 60_000_000);
    assert_eq!(idle["cpu"]["value"]["cpu_micros"], 600_000);
    let wakeups = &idle["wakeups"]["value"];
    assert_eq!(wakeups["total_per_second"], 6.5);
    assert_eq!(wakeups["busiest_thread_per_second"], 5.0);
    assert_eq!(wakeups["without_busiest_per_second"], 1.5);
}

#[test]
fn idle_memory_failure_leaves_the_other_numbers() {
    let mut fake = Fake::new();
    fake.dead_after_window = true;
    let (reports, _) = run(&mut fake);
    let idle = find(&reports, "idle");
    assert_eq!(reason(&idle["memory_at_settle"]), "process exited early");
    assert_eq!(reason(&idle["memory_at_end"]), "process exited early");
    assert_eq!(idle["cpu"]["state"], "measured");
}

#[test]
fn a_failed_prepare_leaves_start_and_idle_not_measured_but_exe_size_runs() {
    let mut fake = Fake::new();
    fake.fail_prepare = true;
    let (reports, names) = run(&mut fake);
    assert!(matches!(names.verdicts[0], Verdict::Failed(_)));
    assert!(matches!(names.verdicts[1], Verdict::NotMeasured(_)));
    assert!(matches!(names.verdicts[3], Verdict::NotMeasured(_)));
    let warm = start(&reports, "warm");
    assert_eq!(reason(&warm["first_usable"]), "fixtures not built");
    assert_eq!(reason(&find(&reports, "idle")["cpu"]), "fixtures not built");
    assert_eq!(find(&reports, "exe-size")["bytes"]["value"], 98_765_432);
    assert!(!fake.events().iter().any(|e| e.starts_with("launch")));
}

#[test]
fn exe_size_is_reported_or_not_measured() {
    let mut fake = Fake::new();
    let (reports, _) = run(&mut fake);
    assert_eq!(find(&reports, "exe-size")["bytes"]["value"], 98_765_432);
    let mut missing = Fake::new();
    missing.no_exe = true;
    let (reports, names) = run(&mut missing);
    assert_eq!(
        reason(&find(&reports, "exe-size")["bytes"]),
        "scratch folder not writable"
    );
    assert!(matches!(names.verdicts[4], Verdict::Failed(4)));
}

#[test]
fn nothing_planted_reaches_the_output() {
    let mut fake = Fake::new();
    let mut names = Names::default();
    let reports = measure_start_and_idle(&mut fake, &mut names);
    let needles = [SECRET, fixture::PRIVATE_FOLDER, fixture::PRIVATE_FILE];
    let scanner = LeakScanner::new(&needles).unwrap();
    let mut sink = Vec::new();
    for report in &reports {
        SafeWriter::new(&mut sink, scanner.clone())
            .write(Output::Json, report)
            .unwrap();
    }
    assert!(sink.len() > 200);
    assert_no_leaks(&sink, &needles).unwrap();
    assert!(matches!(reports[0], Report::Measure(_)));
    let errors = [
        MeasureError::Probe(ProbeError::NoWindow),
        MeasureError::MetricsMissing,
        MeasureError::Io(std::io::ErrorKind::NotFound),
    ];
    for error in errors {
        assert_no_leaks(error.to_string().as_bytes(), &needles).unwrap();
    }
}
