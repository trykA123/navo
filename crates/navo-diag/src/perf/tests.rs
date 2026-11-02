use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{
    FrameSample, GpuFacts, Latency, MonitorFacts, PerfDocument, ProcessSample, RECORD_FOR,
    Recording, Ring, Summary, SystemFacts, UiFlag, UiState, perf_json,
};
use crate::{Identity, Redactor, ReportInput, Salt, write_report_with_extras};

fn redactor() -> Redactor {
    Redactor::new(
        Salt::from_bytes([7; 16]),
        Identity {
            user: Some("canary-user-7731".to_owned()),
            computer: Some("CANARY-PC-7731".to_owned()),
            domain: None,
            profile: Some(r"C:\Users\canary-user-7731".to_owned()),
        },
    )
}

fn frame(work: f32, state: UiState) -> FrameSample {
    FrameSample {
        window: 0,
        at_ms: 0.0,
        interval_ms: None,
        layout_ms: work / 2.0,
        paint_ms: work / 2.0,
        state,
    }
}

fn document(recording: &Recording, system: &SystemFacts) -> Value {
    let ended = recording.started() + Duration::from_secs(20);
    let json = perf_json(&PerfDocument::new(recording, ended, system, &redactor())).unwrap();
    serde_json::from_slice(&json).unwrap()
}

#[test]
fn a_ring_keeps_the_newest_items_in_order() {
    let mut ring = Ring::new(3);
    for item in 1..=5 {
        ring.push(item);
    }
    assert_eq!(ring.iter().copied().collect::<Vec<_>>(), [3, 4, 5]);
    assert_eq!(ring.dropped(), 2);
}

#[test]
fn a_ring_never_grows_past_its_capacity() {
    let mut ring = Ring::new(1000);
    let before = ring.allocated();
    for item in 0..10_000 {
        ring.push(item);
    }
    assert_eq!((ring.len(), ring.allocated()), (1000, before));
}

#[test]
fn a_ring_below_capacity_keeps_everything() {
    let mut ring = Ring::new(8);
    ring.push(1);
    ring.push(2);
    assert_eq!(ring.iter().copied().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(ring.dropped(), 0);
}

#[test]
fn percentiles_use_the_nearest_rank() {
    let summary = Summary::of((1..=100).map(f64::from)).unwrap();
    assert_eq!(
        (
            summary.count,
            summary.p50,
            summary.p95,
            summary.p99,
            summary.max
        ),
        (100, 50.0, 95.0, 99.0, 100.0)
    );
}

#[test]
fn percentiles_of_one_value_are_that_value() {
    let summary = Summary::of([4.5]).unwrap();
    assert_eq!((summary.p50, summary.p99, summary.max), (4.5, 4.5, 4.5));
}

#[test]
fn percentiles_ignore_order_and_non_finite_values() {
    let summary = Summary::of([9.0, f64::NAN, 1.0, 5.0, f64::INFINITY]).unwrap();
    assert_eq!((summary.count, summary.p50, summary.max), (3, 5.0, 9.0));
}

#[test]
fn no_values_give_no_summary() {
    assert_eq!(Summary::of([]), None);
}

#[test]
fn a_recording_is_due_after_two_minutes() {
    let start = Instant::now();
    let recording = Recording::new(start);
    assert_eq!(RECORD_FOR, Duration::from_secs(120));
    assert!(!recording.is_due(start + Duration::from_millis(119_999)));
    assert!(recording.is_due(start + RECORD_FOR));
}

#[test]
fn ui_states_name_their_flags() {
    let state = UiState::default()
        .with(UiFlag::CommandCenter)
        .when(true, UiFlag::Animation)
        .when(false, UiFlag::Scroll);
    assert_eq!(state.label(), "command_center+animation");
    assert_eq!(UiState::default().label(), "plain");
}

#[test]
fn slow_frames_are_counted_per_ui_state() {
    let mut recording = Recording::new(Instant::now());
    let menu = UiState::default().with(UiFlag::ContextMenu);
    for (work, state) in [
        (5.0, menu),
        (20.0, menu),
        (40.0, menu),
        (40.0, UiState::default()),
    ] {
        recording.frames.push(frame(work, state));
    }
    let json = document(&recording, &SystemFacts::default());
    let summary = &json["summary"];
    assert_eq!(summary["slow_frames"]["over_16_7_ms"], 3);
    assert_eq!(summary["slow_frames"]["over_33_ms"], 2);
    let by_menu = &summary["slow_frames_by_state"]["context_menu"];
    assert_eq!(
        (by_menu["frames"].as_u64(), by_menu["over_33_ms"].as_u64()),
        (Some(3), Some(1))
    );
}

#[test]
fn latencies_are_summarised_per_kind() {
    let start = Instant::now();
    let mut recording = Recording::new(start);
    for ms in [10, 20, 30] {
        recording.push_latency(
            Latency::FolderOpen,
            start,
            start + Duration::from_millis(ms),
        );
    }
    let json = document(&recording, &SystemFacts::default());
    let folder = &json["summary"]["latency_ms"]["folder_open"];
    assert_eq!(
        (folder["count"].as_u64(), folder["max"].as_f64()),
        (Some(3), Some(30.0))
    );
    assert_eq!(json["samples"]["latencies"][0]["kind"], "folder_open");
}

#[test]
fn process_samples_are_summarised() {
    let mut recording = Recording::new(Instant::now());
    for cpu in [10.0, 30.0] {
        recording.process.push(ProcessSample {
            cpu_percent: Some(cpu),
            working_set_bytes: Some(1000),
            threads: Some(12),
            ..ProcessSample::default()
        });
    }
    let json = document(&recording, &SystemFacts::default());
    assert_eq!(json["summary"]["cpu_percent_of_one_core"]["max"], 30.0);
    assert_eq!(
        json["samples"]["process"][0]["cpu_percent_of_one_core"],
        10.0
    );
    assert_eq!(json["summary"]["threads_max"], 12);
}

#[test]
fn system_facts_pass_through_the_redactor() {
    let system = SystemFacts {
        os: r"Windows 11 C:\Users\canary-user-7731\Documents\canary-plan-7731.docx".to_owned(),
        cpu_model: "CANARY-PC-7731 cpu".to_owned(),
        gpu: Some(GpuFacts {
            name: r"Adapter at \\canary-srv-7731\share\canary-7731".to_owned(),
            ..GpuFacts::default()
        }),
        monitors: vec![MonitorFacts {
            width_px: 2560,
            height_px: 1440,
            refresh_hz: Some(144.0),
            scale: Some(1.25),
        }],
        ..SystemFacts::default()
    };
    let text = document(&Recording::new(Instant::now()), &system).to_string();
    for needle in ["canary", "7731", "Documents"] {
        assert!(
            !text.to_lowercase().contains(&needle.to_lowercase()),
            "{needle} leaked: {text}"
        );
    }
    assert!(text.contains("2560"));
}

#[test]
fn the_zip_holds_the_report_and_perf_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("perf.zip");
    let perf = perf_json(&PerfDocument::new(
        &Recording::new(Instant::now()),
        Instant::now(),
        &SystemFacts::default(),
        &redactor(),
    ))
    .unwrap();
    write_report_with_extras(
        &path,
        &ReportInput::default(),
        &redactor(),
        &[("perf.json", &perf)],
    )
    .unwrap();
    let mut zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let names: Vec<String> = zip
        .file_names()
        .map(|name| name.unwrap().into_owned())
        .collect();
    assert_eq!(names, ["report.json", "summary.txt", "perf.json"]);
    let mut text = String::new();
    zip.by_name("perf.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["format"], 1);
}

#[test]
fn scroll_samples_do_not_evict_other_latency_kinds() {
    let start = Instant::now();
    let mut recording = Recording::new(start);
    recording.push_latency(
        Latency::FolderOpen,
        start,
        start + Duration::from_millis(40),
    );
    for index in 0..10_000 {
        let at = start + Duration::from_millis(index);
        recording.push_latency(Latency::ScrollPaint, at, at + Duration::from_millis(3));
    }
    let kinds: Vec<Latency> = recording.latencies.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&Latency::FolderOpen));
    assert!(recording.latencies.dropped() > 0);
}

#[test]
fn every_state_label_and_latency_name_is_a_fixed_ascii_identifier() {
    let is_identifier = |text: &str| {
        !text.is_empty()
            && text.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_+".contains(&byte)
            })
    };
    for bits in 0..=u16::from(u8::MAX) {
        let label = UiState(bits).label();
        assert!(is_identifier(&label), "{label:?}");
    }
    for kind in Latency::ALL {
        let name = serde_json::to_value(kind).unwrap();
        let name = name.as_str().unwrap();
        assert!(is_identifier(name) && !name.contains('+'), "{name:?}");
    }
}
