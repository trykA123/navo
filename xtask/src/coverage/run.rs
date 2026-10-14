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
    let report = fs::read_to_string(root.join("target/llvm-cov.json"))
        .map_err(|error| format!("reading target/llvm-cov.json: {error}"))?;
    let floors = fs::read_to_string(root.join("xtask/coverage-floors.toml"))
        .map_err(|error| format!("reading xtask/coverage-floors.toml: {error}"))?;
    let coverage = crate_line_coverage(&report).map_err(|error| error.to_string())?;
    let floors = parse_floors(&floors).map_err(|error| error.to_string())?;
    let missed = misses(&coverage, &floors);
    print_floors(&coverage, &floors);
    print_misses(&missed);
    Ok(if missed.is_empty() {
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

fn print_floors(coverage: &BTreeMap<String, f64>, floors: &BTreeMap<String, f64>) {
    for (crate_name, floor) in floors {
        match coverage.get(crate_name) {
            Some(actual) => println!("{crate_name}: {actual:.1}% (floor {floor}%)"),
            None => println!("{crate_name}: no data (floor {floor}%)"),
        }
    }
}

fn print_misses(missed: &[FloorMiss]) {
    for FloorMiss {
        crate_name,
        actual,
        floor,
    } in missed
    {
        match actual {
            Some(actual) => println!("{crate_name}: {actual:.1}% < {floor}%"),
            None => println!("{crate_name}: no data < {floor}%"),
        }
    }
}
