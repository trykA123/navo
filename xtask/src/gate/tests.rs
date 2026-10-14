use std::collections::BTreeSet;

use super::{GateOptions, Step, steps};

#[test]
fn steps_without_coverage_follow_the_exact_order() {
    let actual: Vec<_> = steps(&GateOptions {
        with_coverage: false,
    })
    .into_iter()
    .map(|step| (step.name, step.program, step.args))
    .collect();
    let expected: [(&str, &str, &[&str]); 9] = [
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
fn steps_without_coverage_have_nine_steps() {
    assert_eq!(
        steps(&GateOptions {
            with_coverage: false,
        })
        .len(),
        9
    );
}

#[test]
fn steps_with_coverage_have_ten_steps() {
    assert_eq!(
        steps(&GateOptions {
            with_coverage: true,
        })
        .len(),
        10
    );
}
