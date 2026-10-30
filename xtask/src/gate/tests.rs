use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{GateOptions, Step, steps};

#[test]
fn steps_without_coverage_follow_the_exact_order() {
    let actual: Vec<_> = steps(&GateOptions {
        with_coverage: false,
    })
    .into_iter()
    .map(|step| (step.name, step.program, step.args))
    .collect();
    let expected: [(&str, &str, &[&str]); 11] = [
        ("fmt", "cargo", &["fmt", "--all", "--check"]),
        (
            "clippy",
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("taplo", "taplo", &["check"]),
        ("typos", "typos", &[]),
        ("size", "cargo", &["xtask", "size"]),
        ("l10n", "cargo", &["xtask", "l10n"]),
        ("colors", "cargo", &["xtask", "colors"]),
        ("deny", "cargo", &["deny", "check"]),
        ("audit", "cargo", &["audit"]),
        (
            "nextest",
            "cargo",
            &["nextest", "run", "--workspace", "--profile", "ci"],
        ),
        ("doc-tests", "cargo", &["test", "--workspace", "--doc"]),
    ];
    let expected: Vec<_> = expected
        .into_iter()
        .map(|(name, program, args)| Step::new(name, program, args))
        .collect();
    let actual: Vec<_> = actual
        .into_iter()
        .map(|(name, program, args)| Step {
            name,
            program,
            args,
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn coverage_is_appended_last_when_requested() {
    let actual = steps(&GateOptions {
        with_coverage: true,
    });
    let mut expected = steps(&GateOptions {
        with_coverage: false,
    });
    expected.push(Step::new("coverage", "cargo", &["xtask", "coverage"]));
    assert_eq!(actual, expected);
}

#[test]
fn no_step_repeats() {
    for with_coverage in [false, true] {
        let actual = steps(&GateOptions { with_coverage });
        let commands: BTreeSet<_> = actual
            .iter()
            .map(|step| (step.program, step.args.clone()))
            .collect();
        assert_eq!(commands.len(), actual.len());
    }
}

#[test]
fn steps_without_coverage_have_eleven_steps() {
    assert_eq!(
        steps(&GateOptions {
            with_coverage: false,
        })
        .len(),
        11
    );
}

#[test]
fn steps_with_coverage_have_twelve_steps() {
    assert_eq!(
        steps(&GateOptions {
            with_coverage: true,
        })
        .len(),
        12
    );
}

#[test]
fn all_steps_passing_return_success() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let steps = [
        Step::new("first", "cargo", &["--version"]),
        Step::new("second", "cargo", &["--version"]),
    ];
    assert_eq!(
        super::run::run_steps(root, &steps),
        std::process::ExitCode::SUCCESS
    );
}

struct TestRoot(std::path::PathBuf);

impl TestRoot {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rusty-gate-{}-{timestamp}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn temporary_roots_have_unique_paths() {
    let first = TestRoot::new();
    let second = TestRoot::new();
    assert_ne!(first.0, second.0);
}

#[test]
fn temporary_root_cleanup_ignores_missing_directories() {
    let root = TestRoot::new();
    std::fs::remove_dir_all(&root.0).unwrap();
    drop(root);
}

#[test]
fn first_failure_prevents_later_steps_from_running() {
    let root = TestRoot::new();
    let steps = [
        Step::new("passing", "cargo", &["--version"]),
        Step::new("failing", "cargo", &["xtask-no-such-subcommand"]),
        Step::new(
            "later",
            "cargo",
            &["new", "--lib", "--vcs", "none", "gate-later-step"],
        ),
    ];
    let marker = root.0.join("gate-later-step/Cargo.toml");
    assert_ne!(
        super::run::run_steps(&root.0, &steps),
        std::process::ExitCode::SUCCESS
    );
    assert!(!marker.exists());
    assert_eq!(
        super::run::run_steps(&root.0, &steps[2..]),
        std::process::ExitCode::SUCCESS
    );
    assert!(marker.is_file());
}

#[test]
fn failing_step_propagates_its_exit_code() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let steps = [Step::new("failing", "cargo", &["xtask-no-such-subcommand"])];
    assert_eq!(
        super::run::run_steps(root, &steps),
        std::process::ExitCode::from(101)
    );
}

#[test]
fn unspawnable_step_returns_failure() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let steps = [Step::new("unspawnable", "rusty-gate-no-such-program", &[])];
    assert_eq!(
        super::run::run_steps(root, &steps),
        std::process::ExitCode::FAILURE
    );
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_string()).collect()
}

#[test]
fn check_steps_for_one_crate_follow_the_exact_order() {
    let actual: Vec<_> = super::check::check_steps(&["rusty-core"])
        .into_iter()
        .map(|step| (step.name, step.program, step.args))
        .collect();
    let expected = [
        ("fmt", "cargo", strings(&["fmt", "--all", "--check"])),
        (
            "clippy",
            "cargo",
            strings(&[
                "clippy",
                "-p",
                "rusty-core",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ]),
        ),
        ("taplo", "taplo", strings(&["check"])),
        ("typos", "typos", strings(&[])),
        ("size", "cargo", strings(&["xtask", "size"])),
        ("l10n", "cargo", strings(&["xtask", "l10n"])),
        ("colors", "cargo", strings(&["xtask", "colors"])),
        (
            "nextest",
            "cargo",
            strings(&["nextest", "run", "-p", "rusty-core", "--profile", "ci"]),
        ),
    ];
    assert_eq!(actual, expected);
}

#[test]
fn check_steps_for_two_crates_repeat_the_package_flag() {
    let steps = super::check::check_steps(&["a", "b"]);
    assert_eq!(
        steps[1].args[..5],
        strings(&["clippy", "-p", "a", "-p", "b"])
    );
    assert_eq!(
        steps[7].args,
        strings(&["nextest", "run", "-p", "a", "-p", "b", "--profile", "ci"])
    );
}

#[test]
fn check_without_crates_is_a_usage_error() {
    assert_eq!(super::check(&[]), std::process::ExitCode::from(2));
    assert_eq!(
        super::check(&strings(&["-p"])),
        std::process::ExitCode::from(2)
    );
    assert_eq!(
        super::check(&strings(&["core"])),
        std::process::ExitCode::from(2)
    );
}
