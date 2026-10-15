use std::fs;
use std::sync::Arc;

use serde_json::Value;
use tempfile::TempDir;

use super::{
    ReportInput, SECTIONS, entries, file_size, input, redactor, report_json, report_text,
    summary_text, write,
};
use crate::write_report;

const PRIVACY_LINE: &str = "This report holds no file names, user or computer names; paths are replaced by tokens like <d:3fa2>.";

fn contains_number(value: &Value, number: f64) -> bool {
    match value {
        Value::Number(n) => n.as_f64() == Some(number),
        Value::Array(items) => items.iter().any(|item| contains_number(item, number)),
        Value::Object(map) => map.values().any(|item| contains_number(item, number)),
        _ => false,
    }
}

#[test]
fn zip_holds_exactly_report_and_summary() {
    let written = write(&input());
    let mut names: Vec<String> = entries(&written.path)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    names.sort();
    assert_eq!(names, ["report.json", "summary.txt"]);
}

#[test]
fn report_json_is_pretty_with_sections_in_order() {
    let written = write(&input());
    let text = report_text(&written.path);
    let positions: Vec<Option<usize>> = SECTIONS
        .iter()
        .map(|section| text.find(&format!("\n  \"{section}\":")))
        .collect();
    let found: Vec<usize> = positions.iter().flatten().copied().collect();
    assert!(
        found.len() == SECTIONS.len() && found.is_sorted(),
        "{positions:?}\n{text}"
    );
}

#[test]
fn report_json_keeps_numbers_as_numbers() {
    let written = write(&input());
    let report = report_json(&written.path);
    assert_eq!(report["system"]["cpu_count"], Value::from(8));
    assert_eq!(
        report["system"]["ram_bytes"],
        Value::from(17_179_869_184_u64)
    );
    assert_eq!(report["system"]["scale"].as_f64(), Some(1.5));
    assert_eq!(report["errors"][0]["os_code"], Value::from(5));
    assert_eq!(report["redaction_misses"], Value::from(3));
    assert!(
        contains_number(&report["memory"], 123_456_789.0),
        "{report:#}"
    );
    assert!(contains_number(&report["performance"], 12.5), "{report:#}");
    assert!(contains_number(&report["performance"], 48.25), "{report:#}");
}

#[test]
fn memory_is_null_when_absent() {
    let written = write(&ReportInput {
        memory: None,
        ..input()
    });
    let report = report_json(&written.path);
    assert_eq!(report.get("memory"), Some(&Value::Null));
}

#[test]
fn events_are_parsed_as_json() {
    let written = write(&input());
    let report = report_json(&written.path);
    assert_eq!(report["recent_events"][1]["fields"]["ms"], Value::from(412));
}

#[test]
fn bad_event_line_is_kept_as_string() {
    let written = write(&ReportInput {
        events: vec![Arc::from("not json {")],
        ..input()
    });
    let report = report_json(&written.path);
    assert_eq!(report["recent_events"][0], Value::from("not json {"));
}

#[test]
fn summary_names_every_section() {
    let written = write(&input());
    let summary = summary_text(&written.path).to_lowercase();
    let missing: Vec<&str> = SECTIONS
        .iter()
        .copied()
        .filter(|section| !summary.contains(&section.replace('_', " ")))
        .collect();
    assert!(missing.is_empty(), "{missing:?}\n{summary}");
}

#[test]
fn summary_ends_with_the_privacy_line() {
    let written = write(&input());
    let summary = summary_text(&written.path);
    assert!(summary.trim_end().ends_with(PRIVACY_LINE), "{summary}");
}

#[test]
fn report_summary_lists_sections_in_order() {
    let written = write(&input());
    assert_eq!(written.summary.sections, SECTIONS);
}

#[test]
fn report_summary_counts_events_and_errors() {
    let written = write(&input());
    assert_eq!((written.summary.events, written.summary.errors), (2, 1));
}

#[test]
fn report_summary_bytes_match_the_zip() {
    let written = write(&input());
    assert_eq!(written.summary.bytes, file_size(&written.path));
}

#[test]
fn write_into_missing_folder_fails() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing").join("report.zip");
    assert!(write_report(&path, &input(), &redactor()).is_err());
}

#[test]
fn failed_write_leaves_nothing_behind() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing").join("report.zip");
    let _ = write_report(&path, &input(), &redactor());
    let left: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert!(left.is_empty(), "{left:?}");
}
