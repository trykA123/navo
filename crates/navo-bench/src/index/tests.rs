use super::*;
use crate::fixture::PRIVATE_QUERY;
use crate::privacy::assert_no_leaks;

fn sample() -> String {
    format!(
        r#"{{
  "cold_start_ms": null, "warm_start_ms": 12.5,
  "timing_tests_command": "cargo nextest run",
  "first_answer_ms": 1.2345, "first_building_answer_ms": null,
  "entries": 5000000, "building": false,
  "queries": [
    {{"query": "{PRIVATE_QUERY}", "runs": 10, "min_ms": 0.5, "median_ms": 0.75, "max_ms": 2.0, "results": 40}},
    {{"query": "{PRIVATE_QUERY} two", "runs": 10, "min_ms": 1.0, "median_ms": 1.5, "max_ms": 3.0, "results": 7}}
  ],
  "memory": {{"busy": false, "parts": [
    {{"name": "index service names", "bytes": 1024, "items": 5}},
    {{"name": "index service private bytes", "bytes": 9000, "items": 0}},
    {{"name": "something unknown", "bytes": 1, "items": 1}}
  ]}}
}}"#
    )
}

#[test]
fn saved_sample_becomes_numbers() {
    let report = parse_index(&sample()).unwrap();
    assert_eq!(report.service, ServiceState::Running);
    assert_eq!(report.cold_start_micros, None);
    assert_eq!(report.warm_start_micros, Some(12_500));
    assert_eq!(report.first_answer_micros, Some(1_235));
    assert_eq!(report.entries, 5_000_000);
    assert_eq!(report.memory_bytes(), Some(9000));
}

#[test]
fn queries_carry_ids_in_order_and_unknown_parts_are_dropped() {
    let report = parse_index(&sample()).unwrap();
    let ids: Vec<_> = report.queries.iter().map(|query| query.id).collect();
    assert_eq!(ids, [0, 1]);
    assert_eq!(report.queries[1].median_micros, 1_500);
    assert_eq!(report.queries[1].results, 7);
    assert_eq!(report.memory.parts.len(), 2);
}

#[test]
fn query_text_never_reaches_the_report() {
    let report = parse_index(&sample()).unwrap();
    let json = serde_json::to_string(&crate::privacy::Report::Index(report)).unwrap();
    assert_no_leaks(json.as_bytes(), &[PRIVATE_QUERY]).unwrap();
}

#[test]
fn garbage_gives_a_code_not_the_input() {
    let error = parse_index(&format!("{PRIVATE_QUERY} not json")).unwrap_err();
    assert_no_leaks(error.to_string().as_bytes(), &[PRIVATE_QUERY]).unwrap();
    assert_eq!(error, ParseError::Json);
}

#[test]
fn index_memory_sample_gives_slots_live_and_parts() {
    let json = r#"{"service_process_id": 42, "parts": [{"name": "index service tree", "bytes": 64, "items": 3}],
        "volumes": [{"slots": 100, "live": 5}], "busy": true, "time": "2026-11-02T10:00:00Z"}"#;
    let memory = parse_memory(json).unwrap();
    assert!(memory.busy);
    assert_eq!(
        memory.volumes,
        [VolumeMemory {
            slots: 100,
            live: 5
        }]
    );
    assert_eq!(memory.parts[0].part, MemoryPartKind::Tree);
    let json = serde_json::to_string(&memory).unwrap();
    assert!(!json.contains("42") && !json.contains("2026"));
}

#[test]
fn old_service_without_volumes_still_parses() {
    let memory = parse_memory(r#"{"parts": [], "volumes": null, "busy": false}"#).unwrap();
    assert!(memory.volumes.is_empty());
}

#[test]
fn unavailable_service_has_no_numbers() {
    let report = IndexReport::unavailable(ServiceState::NotRunning);
    assert_eq!(report.memory_bytes(), None);
    assert!(report.queries.is_empty());
}
