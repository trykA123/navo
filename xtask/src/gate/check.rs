use std::{path::Path, process::ExitCode};

use super::{Step, run::run_labeled};

pub fn run(args: &[String]) -> ExitCode {
    let Some(crates) = parse_crates(args) else {
        eprintln!("usage: cargo xtask check -p <crate> [-p <crate>...]");
        return ExitCode::from(2);
    };
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("check: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    run_labeled(root, "check", &check_steps(&crates))
}

pub(super) fn parse_crates(args: &[String]) -> Option<Vec<&str>> {
    let mut crates = Vec::new();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag != "-p" {
            return None;
        }
        crates.push(args.next()?.as_str());
    }
    (!crates.is_empty()).then_some(crates)
}

pub(super) fn check_steps(crates: &[&str]) -> Vec<Step> {
    let packages: Vec<&str> = crates.iter().flat_map(|name| ["-p", *name]).collect();
    let with = |head: &[&'static str], tail: &[&'static str]| join(head, &packages, tail);
    vec![
        Step::new("fmt", "cargo", &["fmt", "--all", "--check"]),
        Step::new(
            "clippy",
            "cargo",
            &with(&["clippy"], &["--all-targets", "--", "-D", "warnings"]),
        ),
        Step::new("taplo", "taplo", &["check"]),
        Step::new("typos", "typos", &[]),
        Step::new("size", "cargo", &["xtask", "size"]),
        Step::new("l10n", "cargo", &["xtask", "l10n"]),
        Step::new(
            "nextest",
            "cargo",
            &with(&["nextest", "run"], &["--profile", "ci"]),
        ),
    ]
}

fn join<'a>(head: &[&'a str], middle: &[&'a str], tail: &[&'a str]) -> Vec<&'a str> {
    [head, middle, tail].concat()
}
