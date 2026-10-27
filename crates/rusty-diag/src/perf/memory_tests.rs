use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{MemoryTrack, PerfDocument, Recording, SECONDS, SystemFacts, memory_text, perf_json};
use crate::components::ComponentUse;
use crate::heap::HeapStats;
use crate::{Identity, Redactor, Salt};

const MIB: u64 = 1024 * 1024;

fn redactor() -> Redactor {
    Redactor::new(Salt::from_bytes([7; 16]), Identity::default())
}

fn component(name: &'static str, bytes: u64, busy: bool) -> ComponentUse {
    ComponentUse {
        name,
        bytes,
        items: bytes / MIB,
        busy,
    }
}

fn heap(live_mib: u64, peak_mib: u64) -> Option<HeapStats> {
    Some(HeapStats {
        live_bytes: live_mib * MIB,
        peak_bytes: peak_mib * MIB,
        allocations: 900,
    })
}

fn two_seconds(started: Instant) -> MemoryTrack {
    let mut track = MemoryTrack::default();
    track.record(
        started + Duration::from_secs(1),
        heap(100, 120),
        vec![
            component("walk index", 40 * MIB, false),
            component("viewer documents", 8 * MIB, false),
            component("preview body", MIB, false),
        ],
    );
    track.record(
        started + Duration::from_secs(2),
        heap(90, 130),
        vec![
            component("walk index", 0, true),
            component("viewer documents", 16 * MIB, false),
        ],
    );
    track
}

fn perf_value(track: &MemoryTrack, started: Instant) -> Value {
    let recording = Recording::new(started);
    let record = track.record_since(started, &redactor());
    let document = PerfDocument::new(
        &recording,
        started + Duration::from_secs(3),
        &SystemFacts::default(),
        &redactor(),
    )
    .with_memory(record);
    serde_json::from_slice(&perf_json(&document).unwrap()).unwrap()
}

#[test]
fn perf_json_has_heap_and_components_per_sample() {
    let started = Instant::now();
    let value = perf_value(&two_seconds(started), started);
    let samples = &value["samples"]["memory"];
    assert_eq!(samples.as_array().unwrap().len(), 2);
    assert_eq!(samples[0]["at_ms"], json!(1000.0));
    assert_eq!(samples[0]["heap_live_bytes"], json!(100 * MIB));
    assert_eq!(samples[0]["components"]["walk index"], json!(40 * MIB));
    assert_eq!(samples[1]["components"]["walk index"], Value::Null);
    assert_eq!(value["dropped"]["memory"], json!(0));
}

#[test]
fn the_summary_keeps_last_and_max_per_component_largest_first() {
    let started = Instant::now();
    let value = perf_value(&two_seconds(started), started);
    let memory = &value["summary"]["memory"];
    assert_eq!(memory["heap_peak_bytes"], json!(130 * MIB));
    assert_eq!(memory["heap_live_bytes"]["max"], json!((100 * MIB) as f64));
    let components = memory["components"].as_array().unwrap();
    let names: Vec<&str> = components
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["viewer documents", "walk index", "preview body"]);
    assert_eq!(components[0]["last_bytes"], json!(16 * MIB));
    assert_eq!(components[0]["max_bytes"], json!(16 * MIB));
    assert_eq!(components[1]["last_bytes"], Value::Null);
    assert_eq!(components[1]["max_bytes"], json!(40 * MIB));
    assert_eq!(components[1]["busy_samples"], json!(1));
    assert_eq!(components[1]["busy_at_end"], json!(true));
}

#[test]
fn the_text_puts_heap_beside_private_bytes_and_explains_the_rest() {
    let started = Instant::now();
    let record = two_seconds(started).record_since(started, &redactor());
    let text = memory_text(&record, Some(600 * MIB));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "Process private bytes: 600.0 MiB");
    assert_eq!(
        lines[2],
        "Rust heap: 90.0 MiB live, 130.0 MiB peak, 900 allocations"
    );
    assert!(lines[3].starts_with("Outside the Rust heap: 510.0 MiB"));
    let components = &lines[lines.len() - 3..];
    assert_eq!(
        components[0],
        "viewer documents: 16.0 MiB in 16 items (max 16.0 MiB)"
    );
    assert_eq!(
        components[1],
        "walk index: busy at the last sample (max 40.0 MiB)"
    );
    assert_eq!(components[2], "preview body: gone at the end (max 1.0 MiB)");
}

#[test]
fn the_text_says_when_the_heap_is_not_counted() {
    let started = Instant::now();
    let mut track = MemoryTrack::default();
    track.record(started, None, Vec::new());
    let text = memory_text(&track.record_since(started, &redactor()), None);
    assert!(text.contains("Process private bytes: not measured"));
    assert!(text.contains("Rust heap: not counted in this build"));
    assert!(!text.contains("Outside the Rust heap"));
    assert!(text.contains("No component reported its memory."));
}

#[test]
fn the_track_keeps_the_newest_samples_and_counts_dropped_ones() {
    let started = Instant::now();
    let mut track = MemoryTrack::default();
    for second in 0..SECONDS as u64 + 5 {
        track.record(
            started + Duration::from_secs(second),
            heap(second, second),
            Vec::new(),
        );
    }
    let record = track.record_since(started, &redactor());
    assert_eq!((record.samples.len(), record.dropped), (SECONDS, 5));
    assert_eq!(record.samples[0].heap_live_bytes, Some(5 * MIB));
}
