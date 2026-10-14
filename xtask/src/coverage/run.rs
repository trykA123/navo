use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

use super::{FloorMiss, crate_line_coverage, misses, parse_floors};

pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask coverage");
        return ExitCode::from(2);
    }
    match run_coverage() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("coverage: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_coverage() -> Result<ExitCode, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("cannot find repository root")?;
    if !collect_coverage(root)? {
        return Ok(ExitCode::FAILURE);
    }
    check_report(root)
}

pub(super) fn check_report(root: &Path) -> Result<ExitCode, String> {
    let report = fs::read_to_string(root.join("target/llvm-cov.json"))
        .map_err(|error| format!("reading target/llvm-cov.json: {error}"))?;
    let floors = fs::read_to_string(root.join("xtask/coverage-floors.toml"))
        .map_err(|error| format!("reading xtask/coverage-floors.toml: {error}"))?;
    let (lines, passed) = evaluate_report(&report, &floors)?;
    for line in lines {
        println!("{line}");
    }
    Ok(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn collect_coverage(root: &Path) -> Result<bool, String> {
    Command::new("cargo")
        .args([
            "llvm-cov",
            "nextest",
            "--workspace",
            "--json",
            "--output-path",
            "target/llvm-cov.json",
        ])
        .current_dir(root)
        .status()
        .map(|status| status.success())
        .map_err(|error| format!("running cargo llvm-cov nextest: {error}"))
}

pub(super) fn evaluate_report(report: &str, floors: &str) -> Result<(Vec<String>, bool), String> {
    let coverage = crate_line_coverage(report).map_err(|error| error.to_string())?;
    let floors = parse_floors(floors).map_err(|error| error.to_string())?;
    let missed = misses(&coverage, &floors);
    let mut lines = floor_lines(&coverage, &floors);
    lines.extend(miss_lines(&missed));
    Ok((lines, missed.is_empty()))
}

fn floor_lines(coverage: &BTreeMap<String, f64>, floors: &BTreeMap<String, f64>) -> Vec<String> {
    floors
        .iter()
        .map(|(crate_name, floor)| match coverage.get(crate_name) {
            Some(actual) => format!("{crate_name}: {actual:.1}% (floor {floor}%)"),
            None => format!("{crate_name}: no data (floor {floor}%)"),
        })
        .collect()
}

fn miss_lines(missed: &[FloorMiss]) -> Vec<String> {
    missed
        .iter()
        .map(|miss| match miss.actual {
            Some(actual) => format!("{}: {actual:.1}% < {}%", miss.crate_name, miss.floor),
            None => format!("{}: no data < {}%", miss.crate_name, miss.floor),
        })
        .collect()
}
