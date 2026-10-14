use std::collections::BTreeMap;

use super::{CoverageError, FloorMiss, crate_line_coverage, misses, parse_floors};

const REPORT: &str = r#"{
  "data": [{
    "files": [
      {"filename": "/home/u/rusty/crates/rusty-core/src/a.rs",
       "summary": {"lines": {"count": 10, "covered": 5, "percent": 50.0}}},
      {"filename": "/home/u/rusty/crates/rusty-core/src/b.rs",
       "summary": {"lines": {"count": 90, "covered": 90, "percent": 100.0}}},
      {"filename": "/home/u/rusty/xtask/src/main.rs",
       "summary": {"lines": {"count": 4, "covered": 3, "percent": 75.0}}},
      {"filename": "/home/u/.cargo/registry/src/dep/lib.rs",
       "summary": {"lines": {"count": 10, "covered": 0, "percent": 0.0}}}
    ]
  }],
  "type": "llvm.coverage.json.export"
}"#;

fn map(entries: &[(&str, f64)]) -> BTreeMap<String, f64> {
    entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

#[test]
fn line_coverage_sums_counts_per_crate() {
    let got = crate_line_coverage(REPORT).unwrap();
    assert!((got["rusty-core"] - 95.0).abs() < 1e-9);
    assert!((got["xtask"] - 75.0).abs() < 1e-9);
}

#[test]
fn line_coverage_ignores_files_outside_known_crates() {
    let got = crate_line_coverage(REPORT).unwrap();
    assert_eq!(got.len(), 2);
}

#[test]
fn malformed_report_json_is_an_error() {
    assert!(matches!(
        crate_line_coverage("{not json"),
        Err(CoverageError::InvalidReport(_))
    ));
}

#[test]
fn report_without_data_is_an_error() {
    assert!(matches!(
        crate_line_coverage("{}"),
        Err(CoverageError::InvalidReport(_))
    ));
}

#[test]
fn floors_accept_integer_and_float_values() {
    let got = parse_floors("[floors]\nrusty-core = 90\nxtask = 72.5\n").unwrap();
    assert_eq!(got, map(&[("rusty-core", 90.0), ("xtask", 72.5)]));
}

#[test]
fn malformed_floors_toml_is_an_error() {
    assert!(matches!(
        parse_floors("[floors\nrusty-core = "),
        Err(CoverageError::InvalidFloors(_))
    ));
}

#[test]
fn crate_exactly_at_floor_is_not_a_miss() {
    let got = misses(&map(&[("rusty-core", 90.0)]), &map(&[("rusty-core", 90.0)]));
    assert_eq!(got, vec![]);
}

#[test]
fn crate_below_floor_is_a_miss() {
    let got = misses(&map(&[("rusty-core", 89.9)]), &map(&[("rusty-core", 90.0)]));
    assert_eq!(
        got,
        vec![FloorMiss {
            crate_name: "rusty-core".to_string(),
            actual: Some(89.9),
            floor: 90.0
        }]
    );
}

#[test]
fn crate_with_floor_and_no_data_is_a_miss() {
    let got = misses(&map(&[]), &map(&[("rusty-core", 90.0)]));
    assert_eq!(
        got,
        vec![FloorMiss {
            crate_name: "rusty-core".to_string(),
            actual: None,
            floor: 90.0
        }]
    );
}

#[test]
fn crate_with_data_and_no_floor_is_ignored() {
    let got = misses(&map(&[("rusty-core", 10.0)]), &map(&[]));
    assert_eq!(got, vec![]);
}

#[test]
fn crate_with_zero_lines_counts_as_fully_covered() {
    let report = r#"{"data": [{"files": [
      {"filename": "/home/u/rusty/crates/rusty-ui/src/lib.rs",
       "summary": {"lines": {"count": 0, "covered": 0, "percent": 0.0}}}
    ]}]}"#;
    let got = crate_line_coverage(report).unwrap();
    assert!((got["rusty-ui"] - 100.0).abs() < 1e-9);
}
