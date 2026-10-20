use super::{GateOptions, Step};

pub fn steps(options: &GateOptions) -> Vec<Step> {
    let mut steps = vec![
        step("fmt", "cargo", &["fmt", "--all", "--check"]),
        step(
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
        step("taplo", "taplo", &["check"]),
        step("typos", "typos", &[]),
        step("size", "cargo", &["xtask", "size"]),
        step("l10n", "cargo", &["xtask", "l10n"]),
        step("deny", "cargo", &["deny", "check"]),
        step("audit", "cargo", &["audit"]),
        step(
            "nextest",
            "cargo",
            &["nextest", "run", "--workspace", "--profile", "ci"],
        ),
        step("doc-tests", "cargo", &["test", "--workspace", "--doc"]),
    ];
    if options.with_coverage {
        steps.push(step("coverage", "cargo", &["xtask", "coverage"]));
    }
    steps
}

fn step(name: &'static str, program: &'static str, args: &[&str]) -> Step {
    Step::new(name, program, args)
}
