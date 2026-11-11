//! SC11's verdict, calibration line and baseline aggregate, and the `vm` exit codes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::super::{Run, run};
use super::{Baseline, MEMORY_FACTOR, Verdict, aggregate, calibration_line, judge};

/// A full 40-character lowercase sha, the commit the ten calibration runs share.
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A second sha, for the run that must not be aggregated with the first.
const OTHER_SHA: &str = "9876543210abcdef9876543210abcdef98765432";

/// The VM the ten runs were taken on, as `ci.ps1` builds it.
const FINGERPRINT: &str =
    "os=19045.5151;cpus=4;ram=8;gpu=Microsoft+Basic;defender=True;snapshot=2026-11-10a";

/// A second VM, for the fingerprint rule of SC11's verdict.
const OTHER_FINGERPRINT: &str =
    "os=19045.6093;cpus=4;ram=8;gpu=Microsoft+Basic;defender=True;snapshot=2026-11-10a";

/// The `NAVO_COMMIT` of a build of `SHA`: the 12-character short sha.
const COMMIT: &str = "0123456789ab";

/// The `done.json` B4 writes last over one clean run of six launches.
const DONE: &str = r#"{"exits":[0,0,0,0,0,0],"finished":"2026-11-11T04:00:00.0000000Z"}"#;

/// Arguments as `perf-summary` receives them on the command line.
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// SC11's two numbers of one run.
fn numbers(private_bytes: u64, warm_ms: f64) -> Run {
    Run {
        private_bytes,
        warm_ms,
    }
}

/// A calibrated baseline of 40 MB private and 200 ms warm, both guards on.
fn baseline() -> Baseline {
    Baseline {
        vm_fingerprint: FINGERPRINT.to_owned(),
        commit: SHA.to_owned(),
        measured: "2026-11-10".to_owned(),
        reason: "ten idle runs".to_owned(),
        private_bytes: 40_000_000,
        warm_first_usable_ms: 200.0,
        memory_spread: 0.02,
        time_spread: 0.05,
        memory_guard: true,
        time_guard: true,
    }
}

/// One calibration line of the baseline's numbers, on a given commit and VM.
fn one_line(commit: &str, fingerprint: &str) -> String {
    calibration_line(commit, fingerprint, &numbers(40_000_000, 200.0))
}

/// Ten calibration lines whose private bytes span `memory_spread` and whose warm times
/// span `time_spread`, both from the first reading up.
fn lines(memory_spread: f64, time_spread: f64) -> String {
    let memory_step = (40_000_000.0 * memory_spread / 9.0) as u64;
    let time_step = 200.0 * time_spread / 9.0;
    (0..10)
        .map(|index| {
            calibration_line(
                SHA,
                FINGERPRINT,
                &numbers(
                    40_000_000 + (index as u64) * memory_step,
                    200.0 + (index as f64) * time_step,
                ),
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Replace one line of `text` with `line`, so a set can fail on a single reading.
fn replace(text: &str, line: &str) -> String {
    let mut readings: Vec<String> = text.lines().map(str::to_owned).collect();
    readings[0] = line.to_owned();
    readings.join("\n")
}

/// A metrics report of the fixture run that SC10 accepts: `[5000, 200]` panes, 40 MB.
fn report() -> String {
    serde_json::json!({
        "commit": COMMIT,
        "first_usable_ms": 200.0,
        "idle_private_bytes": 40000000,
        "pane_entries": [5000, 200],
    })
    .to_string()
}

/// A run folder of six reports plus `done.json`, as SC10 requires.
struct RunFolder(PathBuf);

impl RunFolder {
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
            "navo-perf-calibrate-{}-{timestamp}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    /// The six reports of one clean run, 40 MB private and 200 ms warm each.
    fn write_run(&self) {
        for name in ["first", "warm-1", "warm-2", "warm-3", "warm-4", "warm-5"] {
            fs::write(self.0.join(format!("{name}.json")), report()).unwrap();
        }
        fs::write(self.0.join("done.json"), DONE).unwrap();
    }
}

impl Drop for RunFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// SC11's `vm` line over `folder`; `fingerprint` is omitted to test the usage error.
fn vm_line(root: &Path, output: &Path, baseline: &Path, fingerprint: Option<&str>) -> Vec<String> {
    let mut line = args(&[
        root.to_str().unwrap(),
        output.to_str().unwrap(),
        "--mode",
        "vm",
        "--commit",
        SHA,
        "--done",
        root.join("done.json").to_str().unwrap(),
        "--baseline",
        baseline.to_str().unwrap(),
    ]);
    if let Some(fingerprint) = fingerprint {
        line.extend(args(&["--fingerprint", fingerprint]));
    }
    line
}

#[test]
fn judge_passes_memory_at_exactly_the_factor() {
    assert_eq!(40_000_000_f64 * MEMORY_FACTOR, 46_000_000.0);
    let verdict = judge(&numbers(46_000_000, 200.0), Some(&baseline()), FINGERPRINT);
    assert!(matches!(verdict, Verdict::Pass(_)), "{verdict:?}");
}

#[test]
fn judge_fails_memory_one_byte_above_the_factor() {
    let verdict = judge(&numbers(46_000_001, 200.0), Some(&baseline()), FINGERPRINT);
    assert!(matches!(verdict, Verdict::Fail(_)), "{verdict:?}");
}

#[test]
fn judge_fails_slow_time_only_under_the_time_guard() {
    let slow = numbers(40_000_000, 250.1);
    let guarded = judge(&slow, Some(&baseline()), FINGERPRINT);
    assert!(matches!(guarded, Verdict::Fail(_)), "{guarded:?}");
    let unguarded = judge(
        &slow,
        Some(&Baseline {
            time_guard: false,
            ..baseline()
        }),
        FINGERPRINT,
    );
    assert!(matches!(unguarded, Verdict::Pass(_)), "{unguarded:?}");
}

#[test]
fn judge_is_not_calibrated_without_a_trustworthy_baseline() {
    let run = numbers(40_000_000, 200.0);
    let no_baseline = judge(&run, None, FINGERPRINT);
    let no_memory_guard = judge(
        &run,
        Some(&Baseline {
            memory_guard: false,
            ..baseline()
        }),
        FINGERPRINT,
    );
    let other_vm = judge(&run, Some(&baseline()), OTHER_FINGERPRINT);
    for verdict in [no_baseline, no_memory_guard, other_vm] {
        assert!(
            matches!(verdict, Verdict::NotCalibrated(_)),
            "judged as {verdict:?}"
        );
    }
}

#[test]
fn calibration_line_is_one_compact_object_of_four_keys() {
    assert_eq!(
        calibration_line(SHA, FINGERPRINT, &numbers(71_000_000, 260.0)),
        format!(
            r#"=== calibration {{"commit":"{SHA}","fingerprint":"{FINGERPRINT}","private_bytes":71000000,"warm_ms":260.0}}"#
        )
    );
}

#[test]
fn aggregate_needs_ten_lines_of_one_commit_and_one_vm() {
    let flat = lines(0.0, 0.0);
    assert!(aggregate(&flat, "2026-11-11", "ten idle runs").is_ok());
    assert!(
        aggregate(&format!("\n{flat}\n\n"), "2026-11-11", "ten idle runs").is_ok(),
        "blank lines are skipped"
    );
    let nine = flat.lines().take(9).collect::<Vec<&str>>().join("\n");
    assert!(
        aggregate(&nine, "2026-11-11", "ten idle runs").is_err(),
        "9 lines"
    );
    let mixed_commits = replace(&flat, &one_line(OTHER_SHA, FINGERPRINT));
    assert!(
        aggregate(&mixed_commits, "2026-11-11", "ten idle runs").is_err(),
        "two commits"
    );
    let mixed_vms = replace(&flat, &one_line(SHA, OTHER_FINGERPRINT));
    assert!(
        aggregate(&mixed_vms, "2026-11-11", "ten idle runs").is_err(),
        "two VMs"
    );
    assert!(aggregate("not a calibration line", "2026-11-11", "r").is_err());
}

#[test]
fn aggregate_guards_on_the_measured_spread() {
    let noisy_memory: Baseline =
        toml::from_str(&aggregate(&lines(0.06, 0.0), "2026-11-11", "noisy").unwrap()).unwrap();
    assert!(!noisy_memory.memory_guard, "6 % memory spread");
    assert!(noisy_memory.time_guard);
    assert_eq!(noisy_memory.private_bytes, 41_199_997);
    let noisy_time: Baseline =
        toml::from_str(&aggregate(&lines(0.0, 0.11), "2026-11-11", "noisy").unwrap()).unwrap();
    assert!(!noisy_time.time_guard, "11 % time spread");
    assert!(noisy_time.memory_guard);
    let steady: Baseline =
        toml::from_str(&aggregate(&lines(0.04, 0.04), "2026-11-11", "steady").unwrap()).unwrap();
    assert!(steady.memory_guard && steady.time_guard);
}

#[test]
fn aggregate_writes_sc11_baseline_in_key_order() {
    let text = aggregate(&lines(0.0, 0.0), "2026-11-11", "ten idle runs").unwrap();
    let parsed: Baseline = toml::from_str(&text).unwrap();
    assert_eq!(
        parsed,
        Baseline {
            vm_fingerprint: FINGERPRINT.to_owned(),
            commit: SHA.to_owned(),
            measured: "2026-11-11".to_owned(),
            reason: "ten idle runs".to_owned(),
            private_bytes: 40_000_000,
            warm_first_usable_ms: 200.0,
            memory_spread: 0.0,
            time_spread: 0.0,
            memory_guard: true,
            time_guard: true,
        }
    );
    let mut previous: Option<usize> = None;
    for key in [
        "vm_fingerprint",
        "commit",
        "measured",
        "reason",
        "private_bytes",
        "warm_first_usable_ms",
        "memory_spread",
        "time_spread",
        "memory_guard",
        "time_guard",
    ] {
        let at = text
            .find(key)
            .unwrap_or_else(|| panic!("no {key} in the baseline:\n{text}"));
        assert!(
            !previous.is_some_and(|before| at <= before),
            "{key} is out of order in the baseline:\n{text}"
        );
        previous = Some(at);
    }
}

#[test]
fn aggregate_output_is_a_baseline_that_denies_unknown_keys() {
    let text = aggregate(&lines(0.0, 0.0), "2026-11-11", "ten idle runs").unwrap();
    assert!(toml::from_str::<Baseline>(&format!("{text}extra = 1\n")).is_err());
}

#[test]
fn run_needs_a_fingerprint_for_vm() {
    let folder = RunFolder::new();
    folder.write_run();
    let output = folder.0.join("summary.md");
    let baseline = folder.0.join("vm-baseline.toml");
    fs::write(
        &baseline,
        aggregate(&lines(0.0, 0.0), "2026-11-11", "ten idle runs").unwrap(),
    )
    .unwrap();
    let code = run(&vm_line(&folder.0, &output, &baseline, None));
    assert_eq!(code, ExitCode::from(2), "--fingerprint is missing");
}

#[test]
fn run_is_not_calibrated_when_the_baseline_file_is_missing() {
    let folder = RunFolder::new();
    folder.write_run();
    let output = folder.0.join("summary.md");
    let code = run(&vm_line(
        &folder.0,
        &output,
        &folder.0.join("vm-baseline.toml"),
        Some(FINGERPRINT),
    ));
    assert_eq!(code, ExitCode::from(4), "no baseline to judge against");
    let summary = fs::read_to_string(&output).unwrap();
    assert!(
        summary.starts_with("perf: not calibrated: "),
        "appended {summary}"
    );
}
