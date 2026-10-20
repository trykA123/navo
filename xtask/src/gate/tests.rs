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
    let expected: [(&str, &str, &[&str]); 10] = [
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
        ("deny", "cargo", &["deny", "check"]),
        ("audit", "cargo", &["audit"]),
        (
            "nextest",
            "cargo",
            &["nextest", "run", "--workspace", "--profile", "ci"],
        ),
        ("doc-tests", "cargo", &["test", "--workspace", "--doc"]),
    ];
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
    expected.push(Step {
        name: "coverage",
        program: "cargo",
        args: &["xtask", "coverage"],
    });
    assert_eq!(actual, expected);
}

#[test]
fn no_step_repeats() {
    for with_coverage in [false, true] {
        let actual = steps(&GateOptions { with_coverage });
        let commands: BTreeSet<_> = actual
            .iter()
            .map(|step| (step.program, step.args))
            .collect();
        assert_eq!(commands.len(), actual.len());
    }
}

#[test]
fn steps_without_coverage_have_ten_steps() {
    assert_eq!(
        steps(&GateOptions {
            with_coverage: false,
        })
        .len(),
        10
    );
}

#[test]
fn steps_with_coverage_have_eleven_steps() {
    assert_eq!(
        steps(&GateOptions {
            with_coverage: true,
        })
        .len(),
        11
    );
}

#[test]
fn all_steps_passing_return_success() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let steps = [
        Step {
            name: "first",
            program: "cargo",
            args: &["--version"],
        },
        Step {
            name: "second",
            program: "cargo",
            args: &["--version"],
        },
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
        Step {
            name: "passing",
            program: "cargo",
            args: &["--version"],
        },
        Step {
            name: "failing",
            program: "cargo",
            args: &["xtask-no-such-subcommand"],
        },
        Step {
            name: "later",
            program: "cargo",
            args: &["new", "--lib", "--vcs", "none", "gate-later-step"],
        },
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
    let steps = [Step {
        name: "failing",
        program: "cargo",
        args: &["xtask-no-such-subcommand"],
    }];
    assert_eq!(
        super::run::run_steps(root, &steps),
        std::process::ExitCode::from(101)
    );
}

#[test]
fn unspawnable_step_returns_failure() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let steps = [Step {
        name: "unspawnable",
        program: "rusty-gate-no-such-program",
        args: &[],
    }];
    assert_eq!(
        super::run::run_steps(root, &steps),
        std::process::ExitCode::FAILURE
    );
}
