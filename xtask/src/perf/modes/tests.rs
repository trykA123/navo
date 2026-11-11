//! SC11's argument rules and SC10's fail-closed checks for the `perf-summary` modes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::{EXPECTED_PANES, Mode, check_report, load_run, parse, run};

/// A full 40-character lowercase sha, as `--commit` must be.
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// The `NAVO_COMMIT` of a build of `SHA`: the 12-character short sha.
const COMMIT: &str = "0123456789ab";

/// Arguments as `perf-summary` receives them on the command line.
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// The arguments of a correct `fresh` call over `root` and `output`.
fn fresh_line(root: &Path, output: &Path) -> Vec<String> {
    args(&[
        root.to_str().unwrap(),
        output.to_str().unwrap(),
        "--mode",
        "fresh",
        "--commit",
        SHA,
    ])
}

/// A report in the real metrics schema, corrected to a fixture run of `SHA`.
fn good() -> serde_json::Value {
    let snapshot =
        include_str!("../../../../crates/navo-app/src/metrics/snapshots/metrics_report.snap");
    let json = snapshot.splitn(3, "---").nth(2).unwrap();
    let mut report: serde_json::Value = serde_json::from_str(json).unwrap();
    report["commit"] = serde_json::json!(COMMIT);
    report["pane_entries"] = serde_json::json!(EXPECTED_PANES);
    report
}

struct TestRoot(PathBuf);

impl TestRoot {
    /// A fresh temp folder, as `bench_walk`'s tests use.
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "navo-perf-modes-{}-{timestamp}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    /// The six reports of one run, with a largest private reading and a middle warm start
    /// that SC11 folds into `71000000` and `260`.
    fn write_run(&self) {
        for (name, private_bytes, first_usable_ms) in [
            ("first", 60_000_000_u64, 300.0_f64),
            ("warm-1", 52_428_800, 240.0),
            ("warm-2", 52_428_800, 250.0),
            ("warm-3", 71_000_000, 260.0),
            ("warm-4", 52_428_800, 270.0),
            ("warm-5", 52_428_800, 280.0),
        ] {
            let mut report = good();
            report["idle_private_bytes"] = serde_json::json!(private_bytes);
            report["first_usable_ms"] = serde_json::json!(first_usable_ms);
            fs::write(self.0.join(format!("{name}.json")), report.to_string()).unwrap();
        }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parse_accepts_a_fresh_line() {
    let parsed = parse(&args(&[
        "reports",
        "summary.md",
        "--mode",
        "fresh",
        "--commit",
        SHA,
    ]));
    match parsed {
        Ok(Mode::Fresh {
            root,
            output,
            commit,
        }) => {
            assert_eq!(root, PathBuf::from("reports"));
            assert_eq!(output, PathBuf::from("summary.md"));
            assert_eq!(commit, SHA);
        }
        other => panic!("a fresh line must parse as fresh, got {other:?}"),
    }
}

#[test]
fn parse_rejects_every_usage_error() {
    let upper = SHA.to_uppercase();
    for line in [
        vec!["reports", "summary.md", "--mode", "fresh", "--commit"],
        vec![
            "reports",
            "summary.md",
            "--mode",
            "fresh",
            "--commit",
            SHA,
            "--baseline",
            "perf/vm-baseline.toml",
        ],
        vec![
            "reports",
            "summary.md",
            "--mode",
            "fresh",
            "--mode",
            "fresh",
            "--commit",
            SHA,
        ],
        vec!["reports", "summary.md", "--mode", "vm", "--commit", SHA],
        vec![
            "reports",
            "summary.md",
            "--mode",
            "fresh",
            "--commit",
            "0123456",
        ],
        vec![
            "reports",
            "summary.md",
            "--mode",
            "fresh",
            "--commit",
            &upper,
        ],
    ] {
        let error = parse(&args(&line)).unwrap_err();
        assert!(error.contains("usage:"), "{line:?} is usage: {error}");
    }
}

#[test]
fn check_report_accepts_a_fixture_run_of_the_tested_commit() {
    let fresh = check_report(&good().to_string(), SHA).unwrap();
    assert_eq!(fresh.private_bytes, 52_428_800);
    assert_eq!(fresh.first_usable_ms, 260.0);
}

#[test]
fn check_report_rejects_a_stale_or_absent_reading() {
    let mut wrong_commit = good();
    wrong_commit["commit"] = serde_json::json!("ffffffffffff");
    let mut short_commit = good();
    short_commit["commit"] = serde_json::json!("0123456");
    let mut absent_memory = good();
    absent_memory
        .as_object_mut()
        .unwrap()
        .remove("idle_private_bytes");
    let mut zero_memory = good();
    zero_memory["idle_private_bytes"] = serde_json::json!(0);
    let mut wrong_panes = good();
    wrong_panes["pane_entries"] = serde_json::json!([5000, 199]);
    let mut absent_panes = good();
    absent_panes.as_object_mut().unwrap().remove("pane_entries");
    for json in [
        wrong_commit.to_string(),
        short_commit.to_string(),
        absent_memory.to_string(),
        zero_memory.to_string(),
        wrong_panes.to_string(),
        absent_panes.to_string(),
        "not JSON".to_owned(),
    ] {
        assert!(check_report(&json, SHA).is_err(), "accepted {json}");
    }
}

#[test]
fn run_appends_the_table_for_six_good_reports() {
    let root = TestRoot::new();
    root.write_run();
    let output = root.0.join("summary.md");
    let folded = load_run(&root.0, SHA).unwrap();
    assert_eq!(folded.private_bytes, 71_000_000);
    assert_eq!(folded.warm_ms, 260.0);
    assert_eq!(
        run(&fresh_line(&root.0, &output)),
        ExitCode::SUCCESS,
        "a fixture run must pass"
    );
    let table = fs::read_to_string(&output).unwrap();
    assert!(table.contains("## Navo Windows performance"), "{table}");
}

#[test]
fn run_fails_when_a_report_is_missing() {
    let root = TestRoot::new();
    root.write_run();
    fs::remove_file(root.0.join("warm-3.json")).unwrap();
    assert_eq!(
        run(&fresh_line(&root.0, &root.0.join("summary.md"))),
        ExitCode::FAILURE,
        "a missing report must fail closed"
    );
}
