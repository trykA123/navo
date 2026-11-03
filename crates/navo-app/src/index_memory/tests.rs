use super::*;

#[test]
fn cli_index_memory_outputs_one_json_object_with_counts_and_bytes() {
    let mut report = MemoryReport {
        process_id: Some(42),
        volumes: Some(vec![VolumeMemory {
            slots: 100,
            live: 5,
        }]),
        ..MemoryReport::default()
    };
    report.add(navo_index::pipe::Part::Names, 1024, 5);
    let text = format_sample(&report, "2026-11-02T10:00:00Z").unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(text.lines().count(), 1);
    assert_eq!(json["service_process_id"], 42);
    assert_eq!(
        json["volumes"],
        serde_json::json!([{ "slots": 100, "live": 5 }])
    );
    assert_eq!(json["parts"][0]["bytes"], 1024);
    assert_eq!(json["parts"][0]["items"], 5);
    assert_eq!(json["time"], "2026-11-02T10:00:00Z");
    assert_eq!(json["busy"], false);
}

#[test]
fn cli_index_memory_accepts_missing_optional_metadata_from_an_old_service() {
    let text = format_sample(&MemoryReport::default(), "now").unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(json["service_process_id"].is_null());
    assert!(json["volumes"].is_null());
}

#[test]
fn cli_index_memory_returns_two_when_the_service_is_absent() {
    assert_eq!(finish(Err(PipeError::NotAvailable)), ExitCode::from(2));
}

#[test]
fn cli_index_memory_shows_build_counters_per_volume() {
    let report = MemoryReport {
        builds: Some(vec![VolumeBuilds {
            full_builds: 2,
            last_reason: Some(navo_index::pipe::BuildReason::JournalReset),
            last_duration_ms: 900,
            unresolved_batches: 4,
            deferred_rebuilds: 1,
            dropped_changes: 6,
        }]),
        ..MemoryReport::default()
    };
    let json: serde_json::Value =
        serde_json::from_str(&format_sample(&report, "now").unwrap()).unwrap();
    assert_eq!(
        json["builds"],
        serde_json::json!([{
            "full_builds_since_start": 2,
            "last_build_reason": "journal_reset",
            "last_build_duration_ms": 900,
            "unresolved_batches": 4,
            "deferred_rebuilds": 1,
            "dropped_changes": 6
        }])
    );
    let old = format_sample(&MemoryReport::default(), "now").unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&old).unwrap()["builds"].is_null());
}
