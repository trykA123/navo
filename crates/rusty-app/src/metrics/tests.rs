use std::time::{Duration, Instant};

use rusty_ui::window_view::StartReport;

use super::memory::Memory;
use super::report::{Build, Measured, Report};
use super::unix::{age_from_proc, parse_peak};
use super::windows::age_from_file_times;
use super::{ProcessClock, Run, mark, marks};

const STATUS: &str = "Name:\tferrite\nVmPeak:\t  900000 kB\nVmHWM:\t   81920 kB\nVmRSS:\t   61440 kB\nRssAnon:\t   40960 kB\n";

fn at(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

fn sample_build() -> Build {
    Build {
        version: "0.4.0".to_owned(),
        commit: "abc123def456".to_owned(),
        exe_bytes: Some(31_457_280),
    }
}

fn sample_measured(start: Instant) -> Measured {
    Measured {
        renderer: Some("Microsoft Basic Render Driver".to_owned()),
        first_frame: at(start, 212),
        first_usable: at(start, 260),
        marks: vec![
            ("main", at(start, 8)),
            ("profile ready", at(start, 11)),
            ("window opening", at(start, 95)),
            ("first frame", at(start, 212)),
            ("first usable", at(start, 260)),
        ],
        memory: Memory {
            private_bytes: Some(52_428_800),
            working_set_bytes: Some(73_400_320),
            peak_working_set_bytes: Some(80_000_000),
        },
    }
}

fn sample_report() -> Report {
    let start = Instant::now();
    Report::new(
        sample_build(),
        &ProcessClock::starting_at(start),
        sample_measured(start),
    )
}

#[test]
fn the_report_schema_is_stable() {
    insta::with_settings!({ snapshot_path => "snapshots", prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("metrics_report", sample_report().to_json());
    });
}

#[test]
fn an_unknown_renderer_and_memory_are_written_as_such() {
    let start = Instant::now();
    let measured = Measured {
        renderer: None,
        memory: Memory::default(),
        ..sample_measured(start)
    };
    let report = Report::new(sample_build(), &ProcessClock::starting_at(start), measured);
    assert_eq!(report.renderer, "unknown");
    assert_eq!(report.idle_private_bytes, None);
    let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert!(value["idle_private_bytes"].is_null());
}

#[test]
fn times_are_milliseconds_from_process_start_to_a_tenth() {
    let start = Instant::now();
    let clock = ProcessClock::starting_at(start);
    assert_eq!(clock.ms(start + Duration::from_micros(212_340)), 212.3);
    assert_eq!(clock.ms(start), 0.0);
}

#[test]
fn the_build_names_this_version_and_executable() {
    let build = Build::current();
    assert_eq!(build.version, env!("CARGO_PKG_VERSION"));
    assert!(!build.commit.is_empty());
    assert!(build.exe_bytes.is_some_and(|bytes| bytes > 0));
}

#[test]
fn the_process_origin_uses_the_os_age() {
    let main_started = Instant::now();
    let called = std::cell::Cell::new(false);
    let clock = ProcessClock::detect(main_started, || {
        called.set(true);
        Some(Duration::from_secs(2))
    });
    assert!(called.get(), "the OS process age was ignored");
    let latest = Instant::now() - Duration::from_secs(2);
    assert!(clock.start() >= main_started - Duration::from_secs(2));
    assert!(clock.start() <= latest);
}

#[test]
fn an_unavailable_os_age_falls_back_to_main() {
    let main_started = Instant::now();
    let clock = ProcessClock::detect(main_started, || None);
    assert_eq!(clock.start(), main_started);
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn the_os_knows_when_this_process_started() {
    let age = super::process_age().expect("the OS start time");
    assert!(age < Duration::from_secs(600), "{age:?}");
}

#[test]
fn proc_stat_gives_the_age_even_with_odd_process_names() {
    let stat = "4242 (fer rite) (x)) S 1 4242 4242 0 -1 4194560 1 0 0 0 0 0 0 0 20 0 1 0 1000 0 0";
    let age = age_from_proc(stat, "12.50 40.00\n").unwrap();
    assert_eq!(age, Duration::from_millis(2_500));
    assert_eq!(age_from_proc("garbage", "12.50 40.00"), None);
    assert_eq!(age_from_proc(stat, "not a number"), None);
}

#[test]
fn file_times_give_the_age_in_hundred_nanosecond_steps() {
    let created = 133_000_000_000_000_000;
    let now = created + 2_125_000;
    assert_eq!(
        age_from_file_times(created, now),
        Some(Duration::from_micros(212_500))
    );
    assert_eq!(age_from_file_times(now, created), None);
}

#[test]
fn proc_status_gives_the_peak_working_set() {
    assert_eq!(parse_peak(STATUS), Some(81_920 * 1024));
    assert_eq!(parse_peak("Name:\tx\n"), None);
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn memory_is_measured_in_this_process() {
    let memory = Memory::sample();
    let (Some(private), Some(working), Some(peak)) = (
        memory.private_bytes,
        memory.working_set_bytes,
        memory.peak_working_set_bytes,
    ) else {
        panic!("memory not measured: {memory:?}");
    };
    assert!(private > 0 && working > 0 && peak >= working);
}

#[test]
fn marks_keep_their_order() {
    mark("test mark one");
    mark("test mark two");
    let labels: Vec<&str> = marks::all().into_iter().map(|(label, _)| label).collect();
    let one = labels.iter().position(|l| *l == "test mark one");
    let two = labels.iter().position(|l| *l == "test mark two");
    assert!(one.is_some() && one < two, "{labels:?}");
}

fn ui_report(start: Instant) -> StartReport {
    StartReport {
        first_frame: at(start, 200),
        first_usable: at(start, 250),
        renderer: None,
        marks: vec![("window built", at(start, 150))],
    }
}

#[test]
fn a_finished_run_writes_the_report_and_exits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metrics.json");
    let start = Instant::now();
    let run = Run::new(path.clone(), ProcessClock::starting_at(start));
    assert!(!run.is_written());
    assert_eq!(run.exit_code(), std::process::ExitCode::FAILURE);
    run.finish(Ok(ui_report(start)));
    assert!(run.is_written());
    assert_eq!(run.exit_code(), std::process::ExitCode::SUCCESS);
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["first_usable_ms"], 250.0);
    let names: Vec<&str> = value["marks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|mark| mark["name"].as_str())
        .collect();
    assert!(names.contains(&"window built"), "{names:?}");
    assert!(names.contains(&"first usable"), "{names:?}");
}

#[test]
fn a_report_that_cannot_be_written_fails_the_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing").join("metrics.json");
    let start = Instant::now();
    let run = Run::new(path, ProcessClock::starting_at(start));
    run.finish(Ok(ui_report(start)));
    assert!(!run.is_written());
    assert_eq!(run.exit_code(), std::process::ExitCode::FAILURE);
}

#[test]
fn a_listing_failure_never_writes_a_successful_report() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metrics.json");
    let run = Run::new(path.clone(), ProcessClock::starting_at(Instant::now()));
    run.finish(Err("cannot list folder: access denied".to_owned()));
    assert_eq!(run.exit_code(), std::process::ExitCode::FAILURE);
    assert!(!run.is_written());
    assert!(!path.exists());
}
