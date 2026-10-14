use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    CoverageError, FloorMiss, crate_line_coverage, crate_line_coverage_in, misses, parse_floors,
};

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
fn coverage_in_a_checkout_under_crates_uses_repo_relative_paths() {
    let report = REPORT.replace("/home/u/rusty", "/home/u/crates/rusty");
    for root in ["/home/u/crates/rusty", "/home/u/crates/rusty/"] {
        assert_eq!(
            crate_line_coverage_in(&report, Path::new(root)).unwrap(),
            map(&[("rusty-core", 95.0), ("xtask", 75.0)])
        );
    }
}

#[test]
fn coverage_in_a_repo_ignores_outside_files_and_similar_root_prefixes() {
    let report = r#"{"data":[{"files":[
        {"filename":"/home/u/other/crates/rusty-core/src/a.rs"},
        {"filename":"/home/u/rusty-other/xtask/src/main.rs"},
        {"filename":"/home/u/rusty"}]}]}"#;
    assert!(
        crate_line_coverage_in(report, Path::new("/home/u/rusty"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn coverage_in_a_windows_checkout_normalizes_roots_and_filenames() {
    let report = r#"{"data":[{"files":[
        {"filename":"C:\\work\\xtask\\rusty\\crates\\rusty-core\\src\\a.rs",
         "summary":{"lines":{"count":4,"covered":3}}},
        {"filename":"C:/work/xtask/rusty/xtask/src/main.rs",
         "summary":{"lines":{"count":2,"covered":1}}}]}]}"#;
    for root in [r"C:\work\xtask\rusty", r"C:\work\xtask\rusty\"] {
        assert_eq!(
            crate_line_coverage_in(report, Path::new(root)).unwrap(),
            map(&[("rusty-core", 75.0), ("xtask", 50.0)])
        );
    }
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

#[test]
fn invalid_floors_display_the_error_message() {
    let error = CoverageError::InvalidFloors("invalid floor value".to_owned());
    assert_eq!(error.to_string(), "invalid floor value");
}

#[test]
fn invalid_reports_display_the_error_message() {
    let error = CoverageError::InvalidReport("invalid coverage report".to_owned());
    assert_eq!(error.to_string(), "invalid coverage report");
}

#[test]
fn report_file_without_a_filename_is_an_error() {
    let report = r#"{"data":[{"files":[{}]}]}"#;
    assert_eq!(
        crate_line_coverage(report),
        Err(CoverageError::InvalidReport(
            "missing file filename".to_owned()
        ))
    );
}

#[test]
fn report_file_without_summary_lines_is_an_error() {
    let report = r#"{"data":[{"files":[{"filename":"xtask/src/main.rs"}]}]}"#;
    assert_eq!(
        crate_line_coverage(report),
        Err(CoverageError::InvalidReport(
            "xtask/src/main.rs: missing summary.lines".to_owned()
        ))
    );
}

#[test]
fn report_file_with_invalid_line_counts_is_an_error() {
    let report = r#"{"data":[{"files":[{"filename":"xtask/src/main.rs",
        "summary":{"lines":{"count":"invalid","covered":1}}}]}]}"#;
    let error = crate_line_coverage(report).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("xtask/src/main.rs: invalid type:")
    );
}

#[test]
fn report_paths_without_a_crate_name_are_ignored() {
    let report = r#"{"data":[{"files":[
        {"filename":"/repo/crates//src/lib.rs"},
        {"filename":"/repo/crates/"},
        {"filename":"/repo/crates"}]}]}"#;
    assert!(crate_line_coverage(report).unwrap().is_empty());
}

#[test]
fn report_paths_with_windows_separators_identify_the_crate() {
    let report = r#"{"data":[{"files":[{"filename":"C:\\repo\\crates\\rusty-core\\src\\lib.rs",
        "summary":{"lines":{"count":4,"covered":3}}}]}]}"#;
    assert_eq!(
        crate_line_coverage(report).unwrap(),
        map(&[("rusty-core", 75.0)])
    );
}

#[test]
fn passing_report_prints_each_floor_in_name_order() {
    let floors = "[floors]\nxtask = 72.5\nrusty-core = 95\n";
    assert_eq!(
        super::run::evaluate_report(REPORT, floors).unwrap(),
        (
            vec![
                "rusty-core: 95.0% (floor 95%)".to_owned(),
                "xtask: 75.0% (floor 72.5%)".to_owned(),
            ],
            true
        )
    );
}

#[test]
fn failing_report_prints_floors_then_measured_and_missing_crate_misses() {
    let floors = "[floors]\nxtask = 75\nrusty-core = 96\nrusty-fs = 85\n";
    assert_eq!(
        super::run::evaluate_report(REPORT, floors).unwrap(),
        (
            vec![
                "rusty-core: 95.0% (floor 96%)".to_owned(),
                "rusty-fs: no data (floor 85%)".to_owned(),
                "xtask: 75.0% (floor 75%)".to_owned(),
                "rusty-core: 95.0% < 96%".to_owned(),
                "rusty-fs: no data < 85%".to_owned(),
            ],
            false
        )
    );
}

#[test]
fn report_without_floors_passes_without_output() {
    assert_eq!(
        super::run::evaluate_report(REPORT, "[floors]").unwrap(),
        (Vec::new(), true)
    );
}

#[test]
fn report_evaluation_propagates_invalid_report_errors() {
    let expected = crate_line_coverage("bad report").unwrap_err().to_string();
    assert_eq!(
        super::run::evaluate_report("bad report", "[floors]"),
        Err(expected)
    );
}

#[test]
fn report_evaluation_propagates_invalid_floor_errors() {
    let expected = parse_floors("bad floors").unwrap_err().to_string();
    assert_eq!(
        super::run::evaluate_report(REPORT, "bad floors"),
        Err(expected)
    );
}

struct TestReport(std::path::PathBuf);

impl TestReport {
    fn new(floors: &str) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rusty-coverage-{}-{timestamp}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::create_dir(root.join("xtask")).unwrap();
        let mut report: serde_json::Value = serde_json::from_str(REPORT).unwrap();
        let prefix = root.to_string_lossy().replace('\\', "/");
        for file in report["data"][0]["files"].as_array_mut().unwrap() {
            let filename = file["filename"].as_str().unwrap();
            file["filename"] = filename.replace("/home/u/rusty", &prefix).into();
        }
        std::fs::write(root.join("target/llvm-cov.json"), report.to_string()).unwrap();
        std::fs::write(root.join("xtask/coverage-floors.toml"), floors).unwrap();
        Self(root)
    }
}

impl Drop for TestReport {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn temporary_reports_have_unique_paths() {
    let first = TestReport::new("[floors]");
    let second = TestReport::new("[floors]");
    assert_ne!(first.0, second.0);
}

#[test]
fn temporary_report_cleanup_ignores_missing_directories() {
    let report = TestReport::new("[floors]");
    std::fs::remove_dir_all(&report.0).unwrap();
    drop(report);
}

#[test]
fn saved_report_at_the_floor_returns_success() {
    let report = TestReport::new("[floors]\nxtask = 75\n");
    assert_eq!(
        super::run::check_report(&report.0).unwrap(),
        std::process::ExitCode::SUCCESS
    );
}

#[test]
fn saved_report_below_the_floor_returns_failure() {
    let report = TestReport::new("[floors]\nxtask = 85\n");
    assert_eq!(
        super::run::check_report(&report.0).unwrap(),
        std::process::ExitCode::FAILURE
    );
}

#[test]
fn saved_report_read_errors_identify_the_report_path() {
    let report = TestReport::new("[floors]");
    std::fs::remove_file(report.0.join("target/llvm-cov.json")).unwrap();
    let error = super::run::check_report(&report.0).unwrap_err();
    assert!(error.starts_with("reading target/llvm-cov.json:"));
}

#[test]
fn saved_floor_read_errors_identify_the_floors_path() {
    let report = TestReport::new("[floors]");
    std::fs::remove_file(report.0.join("xtask/coverage-floors.toml")).unwrap();
    let error = super::run::check_report(&report.0).unwrap_err();
    assert!(error.starts_with("reading xtask/coverage-floors.toml:"));
}

#[test]
fn saved_report_propagates_evaluation_errors() {
    let report = TestReport::new("invalid floors");
    let expected = parse_floors("invalid floors").unwrap_err().to_string();
    assert_eq!(super::run::check_report(&report.0), Err(expected));
}
