use rusty_index::pipe::{Hit, Part, QueryTerm, Status, VolumeStatus};

use super::*;

fn measured(building: bool) -> Measurement {
    let mut calls = 0;
    let mut memory = MemoryReport::default();
    memory.add(Part::Names, 4096, 42);
    memory.add(Part::FileIds, 2048, 42);
    memory.busy = true;
    let report = measure(
        |request| {
            if calls == 0 {
                assert_eq!(request, Request::default());
            } else {
                assert_eq!(request, queries()[(calls - 1) / RUNS].1);
            }
            let elapsed = if calls == 0 {
                12
            } else {
                10 - (calls - 1) % RUNS
            };
            calls += 1;
            Ok((
                Reply {
                    hits: vec![Hit {
                        path: "C:\\windows.dll".into(),
                        flags: 0,
                        score: 1,
                        indices: vec![],
                    }],
                    status: Status {
                        volumes: vec![VolumeStatus {
                            entries: 42,
                            progress: (building && calls == 2).then_some(0),
                            ..VolumeStatus::default()
                        }],
                    },
                    ..Reply::default()
                },
                Duration::from_millis(elapsed as u64),
            ))
        },
        || Ok(memory),
    )
    .unwrap();
    assert_eq!(calls, 1 + queries().len() * RUNS);
    report
}

#[test]
fn fake_measurement_reports_ten_round_trips_and_building_observations() {
    let report = measured(true);
    assert_eq!(report.first_answer_ms, 12.0);
    assert_eq!(report.first_building_answer_ms, Some(10.0));
    assert_eq!(report.entries, 42);
    assert!(!report.building);
    assert_eq!(report.queries.len(), 5);
    for query in &report.queries {
        assert_eq!(query.runs, 10);
        assert_eq!(
            (query.min_ms, query.median_ms, query.max_ms),
            (1.0, 5.5, 10.0)
        );
        assert_eq!(query.results, 1);
    }
    assert_eq!(queries()[2].1.terms, [QueryTerm::Ext(vec!["dll".into()])]);
    assert!(report.memory.busy);
}

#[test]
fn fake_measurement_formats_as_pasteable_text() {
    let text = format_report(&measured(true), false).unwrap();
    for expected in [
        "cold/warm start: run the timing tests",
        TIMING_COMMAND,
        "first answer (existing service): 12.000 ms",
        "first answer while building (observed): 10.000 ms",
        "entries: 42",
        "windows: min/median/max 1.000/5.500/10.000 ms",
        "*.dll: min/median/max",
        "index service names: 4096 bytes",
        "memory (bytes; partial: true)",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

#[test]
fn fake_measurement_formats_as_json_with_explicit_unmeasured_start_times() {
    let json: serde_json::Value =
        serde_json::from_str(&format_report(&measured(true), true).unwrap()).unwrap();
    assert!(json["cold_start_ms"].is_null());
    assert!(json["warm_start_ms"].is_null());
    assert_eq!(json["timing_tests_command"], TIMING_COMMAND);
    assert_eq!(json["first_building_answer_ms"], 10.0);
    assert_eq!(json["entries"], 42);
    assert_eq!(json["queries"].as_array().unwrap().len(), 5);
    assert_eq!(json["queries"][0]["runs"], 10);
    assert_eq!(json["queries"][0]["median_ms"], 5.5);
    assert_eq!(json["memory"]["parts"][0]["bytes"], 4096);
    assert_eq!(json["memory"]["parts"][0]["name"], Part::Names.label());
    assert_eq!(json["memory"]["busy"], true);
}

#[test]
fn a_ready_service_does_not_invent_a_building_answer() {
    let report = measured(false);
    assert_eq!(report.first_building_answer_ms, None);
    let text = format_report(&report, false).unwrap();
    assert!(text.contains("first answer while building: not observed"));
    let json: serde_json::Value =
        serde_json::from_str(&format_report(&report, true).unwrap()).unwrap();
    assert!(json["first_building_answer_ms"].is_null());
}

#[test]
fn an_absent_or_stopped_service_returns_one_line_and_a_failure_exit() {
    for running in [None, Some(false)] {
        let error = execute(false, || {
            require_running(running)?;
            panic!("an unavailable service must not connect");
        })
        .unwrap_err();
        assert_eq!(error.lines().count(), 1);
        assert!(error.contains(if running.is_none() {
            "not installed"
        } else {
            "not running"
        }));
        assert_eq!(finish(Err(error)), ExitCode::FAILURE);
    }
}

#[test]
fn a_pipe_failure_stops_measurement_without_reading_memory() {
    let error = measure(
        |_| Err("disconnected".into()),
        || panic!("a failed connection must stop measurement"),
    )
    .unwrap_err();
    assert_eq!(error, "disconnected");
}
