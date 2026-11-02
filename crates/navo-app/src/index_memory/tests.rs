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
