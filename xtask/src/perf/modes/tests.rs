//! SC11's argument rules and SC10's fail-closed checks for the `perf-summary` modes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::{EXPECTED_PANES, Mode, check_done, check_report, load_run, parse, run};

/// A full 40-character lowercase sha, as `--commit` must be.
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// The `NAVO_COMMIT` of a build of `SHA`: the 12-character short sha.
const COMMIT: &str = "0123456789ab";

/// The `done.json` B4 writes last over one clean run of six launches.
const DONE: &str = r#"{"exits":[0,0,0,0,0,0],"finished":"2026-11-11T04:00:00Z"}"#;

/// `done.json` of a run whose sixth launch hit B4's 60 s timeout.
const DONE_124: &str = r#"{"exits":[0,0,0,0,0,124],"finished":"2026-11-11T04:00:00Z"}"#;

/// SC11's fingerprint of this VM, as `ci.ps1` builds it.
const FP: &str = "os=19045.5151;cpus=4";

/// SC11's calibration line for the run `TestRoot::write_run` folds under `--commit SHA`.
const CALIBRATION: &str = r#"=== calibration {"commit":"0123456789abcdef0123456789abcdef01234567","fingerprint":"os=19045.5151;cpus=4","private_bytes":71000000,"warm_ms":260.0}"#;

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

/// The arguments of a line of mode `name`, over the two run positionals and `tail`.
fn run_line(name: &str, tail: &[&str]) -> Vec<String> {
    let mut values = vec!["reports", "summary.md", "--mode", name];
    values.extend_from_slice(tail);
    args(&values)
}

/// The arguments of a line of mode `name` over a temp run folder and summary file.
fn mode_line(name: &str, root: &Path, output: &Path, tail: &[&str]) -> Vec<String> {
    let mut values = vec![
        root.to_str().unwrap(),
        output.to_str().unwrap(),
        "--mode",
        name,
    ];
    values.extend_from_slice(tail);
    args(&values)
}

/// SC11's `vm` line over `root`, judged against `baseline` on fingerprint `FP`.
fn vm_line(root: &Path, output: &Path, baseline: &Path) -> Vec<String> {
    mode_line(
        "vm",
        root,
        output,
        &[
            "--commit",
            SHA,
            "--done",
            root.join("done.json").to_str().unwrap(),
            "--baseline",
            baseline.to_str().unwrap(),
            "--fingerprint",
            FP,
        ],
    )
}

/// SC11's `calibrate` line over `root`, which carries no baseline.
fn calibrate_line(root: &Path, output: &Path) -> Vec<String> {
    mode_line(
        "calibrate",
        root,
        output,
        &[
            "--commit",
            SHA,
            "--done",
            root.join("done.json").to_str().unwrap(),
            "--fingerprint",
            FP,
        ],
    )
}

/// SC11's `aggregate` line over a lines file: it takes one positional, not a run folder.
fn aggregate_line(lines: &Path) -> Vec<String> {
    args(&[
        "--mode",
        "aggregate",
        lines.to_str().unwrap(),
        "--measured",
        "2026-11-11",
        "--reason",
        "ten idle runs",
    ])
}

/// A `vm-baseline.toml` on fingerprint `FP` whose memory limit gates `private_bytes`, at
/// the SC11 baseline of 300 ms warm with both guards on.
fn write_baseline(path: &Path, private_bytes: u64) {
    fs::write(
        path,
        format!(
            r#"vm_fingerprint = "{FP}"
commit = "{SHA}"
measured = "2026-11-10"
reason = "ten idle runs"
private_bytes = {private_bytes}
warm_first_usable_ms = 300.0
memory_spread = 0.02
time_spread = 0.05
memory_guard = true
time_guard = true
"#
        ),
    )
    .unwrap();
}

/// Ten calibration lines of one commit and one VM, as `calibrate` writes them.
fn calibration_lines() -> String {
    format!("{CALIBRATION}\n").repeat(10)
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

    /// `done.json`, as B4 writes it last over the run.
    fn write_done(&self, json: &str) {
        fs::write(self.0.join("done.json"), json).unwrap();
    }

    /// The summary file SC11's modes append their line to.
    fn summary(&self) -> PathBuf {
        self.0.join("summary.md")
    }

    /// The `vm-baseline.toml` beside the run.
    fn baseline(&self) -> PathBuf {
        self.0.join("vm-baseline.toml")
    }

    /// The contents of `path`, which the appended-line tests read back.
    fn read(&self, path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
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

#[test]
fn parse_accepts_the_modes_of_a_vm_run() {
    let vm = parse(&args(&[
        "reports",
        "summary.md",
        "--mode",
        "vm",
        "--commit",
        SHA,
        "--done",
        "done.json",
        "--baseline",
        "perf/vm-baseline.toml",
        "--fingerprint",
        "os=19045.5151;cpus=4",
    ]));
    match vm {
        Ok(Mode::Vm {
            run: args,
            baseline,
        }) => {
            assert_eq!(args.root, PathBuf::from("reports"));
            assert_eq!(args.output, PathBuf::from("summary.md"));
            assert_eq!(args.commit, SHA);
            assert_eq!(args.done, PathBuf::from("done.json"));
            assert_eq!(args.fingerprint, "os=19045.5151;cpus=4");
            assert_eq!(baseline, PathBuf::from("perf/vm-baseline.toml"));
        }
        other => panic!("a vm line must parse as vm, got {other:?}"),
    }
    let aggregate = parse(&args(&[
        "--mode",
        "aggregate",
        "calibration.txt",
        "--measured",
        "2026-11-11",
        "--reason",
        "ten idle runs",
    ]));
    match aggregate {
        Ok(Mode::Aggregate {
            lines,
            measured,
            reason,
        }) => {
            assert_eq!(lines, PathBuf::from("calibration.txt"));
            assert_eq!(measured, "2026-11-11");
            assert_eq!(reason, "ten idle runs");
        }
        other => panic!("an aggregate line must parse as aggregate, got {other:?}"),
    }
}

#[test]
fn parse_rejects_a_flag_or_positional_of_another_mode() {
    for line in [
        run_line(
            "vm",
            &[
                "--commit",
                SHA,
                "--done",
                "done.json",
                "--fingerprint",
                "fp",
            ],
        ),
        run_line(
            "vm",
            &[
                "--commit",
                SHA,
                "--done",
                "done.json",
                "--baseline",
                "b",
                "--fingerprint",
            ],
        ),
        run_line(
            "calibrate",
            &[
                "--commit",
                SHA,
                "--done",
                "d",
                "--fingerprint",
                "fp",
                "--baseline",
                "b",
            ],
        ),
        args(&[
            "--mode",
            "aggregate",
            "--measured",
            "2026-11-11",
            "--reason",
            "r",
        ]),
        run_line("aggregate", &["--measured", "2026-11-11", "--reason", "r"]),
    ] {
        let error = parse(&line).unwrap_err();
        assert!(error.contains("usage:"), "{line:?} is usage: {error}");
    }
}

#[test]
fn check_done_accepts_six_zero_exits() {
    check_done(DONE).unwrap();
}

#[test]
fn check_done_rejects_anything_else() {
    for json in [
        r#"{"exits":[],"finished":"2026-11-11T04:00:00Z"}"#,
        r#"{"exits":[0,0,0,0,0],"finished":"2026-11-11T04:00:00Z"}"#,
        r#"{"exits":[0,0,0,0,0,0,0],"finished":"2026-11-11T04:00:00Z"}"#,
        r#"{"exits":[0,0,0,0,0,1],"finished":"2026-11-11T04:00:00Z"}"#,
        r#"{"exits":["0",0,0,0,0,0],"finished":"2026-11-11T04:00:00Z"}"#,
        r#"{"exits":[0,0,0.5,0,0,0],"finished":"2026-11-11T04:00:00Z"}"#,
        r#"{"exits":[0,0,0,0,0,0],"finished":"2026-11-11T04:00:00Z","extra":true}"#,
        r#"{"exits":[0,0,0,0,0,0]}"#,
        "not JSON",
    ] {
        assert!(check_done(json).is_err(), "accepted {json}");
    }
}

#[test]
fn check_done_rejects_a_finished_that_is_not_a_string() {
    for json in [
        r#"{"exits":[0,0,0,0,0,0],"finished":null}"#,
        r#"{"exits":[0,0,0,0,0,0],"finished":1762833600}"#,
        r#"{"exits":[0,0,0,0,0,0],"finished":["2026-11-11T04:00:00Z"]}"#,
    ] {
        assert!(check_done(json).is_err(), "accepted {json}");
    }
    check_done(DONE).unwrap();
}

#[test]
fn vm_fails_closed_on_a_timed_out_launch() {
    let root = TestRoot::new();
    root.write_run();
    root.write_done(DONE_124);
    write_baseline(&root.baseline(), 62_000_000);
    assert_eq!(
        run(&vm_line(&root.0, &root.summary(), &root.baseline())),
        ExitCode::from(1),
        "done.json exited 124, so the run never finished"
    );
}

#[test]
fn calibrate_fails_closed_on_a_timed_out_launch() {
    let root = TestRoot::new();
    root.write_run();
    root.write_done(DONE_124);
    assert_eq!(
        run(&calibrate_line(&root.0, &root.summary())),
        ExitCode::from(1),
        "a timed out launch must not calibrate"
    );
}

#[test]
fn vm_fails_closed_when_done_json_is_absent() {
    let root = TestRoot::new();
    root.write_run();
    write_baseline(&root.baseline(), 62_000_000);
    assert_eq!(
        run(&vm_line(&root.0, &root.summary(), &root.baseline())),
        ExitCode::from(1),
        "no done.json means no finished run"
    );
}

#[test]
fn calibrate_fails_closed_when_done_json_is_absent() {
    let root = TestRoot::new();
    root.write_run();
    assert_eq!(
        run(&calibrate_line(&root.0, &root.summary())),
        ExitCode::from(1),
        "no done.json means no calibration line"
    );
}

#[test]
fn vm_fails_on_a_baseline_it_cannot_read() {
    let root = TestRoot::new();
    root.write_run();
    root.write_done(DONE);
    let summary = root.summary();
    let baseline = root.baseline();
    write_baseline(&baseline, 62_000_000);
    let good = root.read(&baseline);
    let extra_key = format!("{good}extra = 1\n");
    for text in ["not a baseline", extra_key.as_str()] {
        fs::write(&baseline, text).unwrap();
        assert_eq!(
            run(&vm_line(&root.0, &summary, &baseline)),
            ExitCode::from(1),
            "an unreadable baseline is SC11's failure, not its exit 4"
        );
    }
}

#[test]
fn vm_passes_a_steady_run_and_appends_its_line() {
    let root = TestRoot::new();
    root.write_run();
    root.write_done(DONE);
    write_baseline(&root.baseline(), 62_000_000);
    let summary = root.summary();
    assert_eq!(
        run(&vm_line(&root.0, &summary, &root.baseline())),
        ExitCode::SUCCESS,
        "71000000 bytes is under the baseline's 1.15"
    );
    let text = root.read(&summary);
    assert!(text.starts_with("perf: pass: "), "appended {text}");
}

#[test]
fn vm_fails_a_memory_regression_and_appends_its_line() {
    let root = TestRoot::new();
    root.write_run();
    root.write_done(DONE);
    write_baseline(&root.baseline(), 60_000_000);
    let summary = root.summary();
    assert_eq!(
        run(&vm_line(&root.0, &summary, &root.baseline())),
        ExitCode::FAILURE,
        "71000000 bytes is over the baseline's 1.15"
    );
    let text = root.read(&summary);
    assert!(text.starts_with("perf: FAIL: "), "appended {text}");
}

#[test]
fn calibrate_appends_its_calibration_line() {
    let root = TestRoot::new();
    root.write_run();
    root.write_done(DONE);
    let summary = root.summary();
    assert_eq!(
        run(&calibrate_line(&root.0, &summary)),
        ExitCode::SUCCESS,
        "a clean run calibrates"
    );
    assert_eq!(root.read(&summary).trim_end(), CALIBRATION);
}

#[test]
fn aggregate_reads_ten_lines_and_rejects_others() {
    let root = TestRoot::new();
    let lines = root.0.join("calibration.txt");
    fs::write(&lines, calibration_lines()).unwrap();
    assert_eq!(
        run(&aggregate_line(&lines)),
        ExitCode::SUCCESS,
        "ten lines make a baseline"
    );
    fs::write(&lines, "not a calibration line\n").unwrap();
    assert_eq!(
        run(&aggregate_line(&lines)),
        ExitCode::FAILURE,
        "a lines file of other text fails"
    );
}
