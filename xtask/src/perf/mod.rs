use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

mod fixture;
mod report;

#[cfg(test)]
mod tests;

pub(crate) fn fixture(args: &[String]) -> ExitCode {
    let [root] = args else {
        eprintln!("usage: cargo xtask perf-fixture <dir>");
        return ExitCode::from(2);
    };
    println!("perf-fixture: generating A (5,000 files), B (200 folders of 25 files) in {root}");
    finish(fixture::generate(Path::new(root)))
}

pub(crate) fn summary(args: &[String]) -> ExitCode {
    let [root, output] = args else {
        eprintln!("usage: cargo xtask perf-summary <reports-dir> <summary-file>");
        return ExitCode::from(2);
    };
    finish(summarize(Path::new(root), Path::new(output)))
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("perf: {error}");
            ExitCode::FAILURE
        }
    }
}

fn load(root: &Path, name: &str) -> Result<report::Report, String> {
    let path = root.join(format!("{name}.json"));
    let json = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    report::parse(&json).map_err(|error| format!("{}: {error}", path.display()))
}

fn summarize(root: &Path, output: &Path) -> Result<(), String> {
    let first = load(root, "first")?;
    let warm = (1..=5)
        .map(|index| load(root, &format!("warm-{index}")))
        .collect::<Result<Vec<_>, _>>()?;
    let guard = report::guard(&first, &warm);
    let markdown = report::markdown(&first, &warm, &guard);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)
        .and_then(|mut file| file.write_all(markdown.as_bytes()))
        .map_err(|error| format!("{}: {error}", output.display()))?;
    guard
}
