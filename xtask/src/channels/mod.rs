//! Channel ratchet (AM7): counts unbounded channel constructions in tracked
//! Rust outside test code and fails when the count rises above the baseline.
//! Runs as `cargo xtask channels` and as a step of the gate.

mod skip;

#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

/// Baseline file, relative to the repository root.
const BASELINE_FILE: &str = "xtask/channels-baseline.txt";

/// Substrings that mark an unbounded channel construction.
const PATTERNS: [&str; 3] = ["mpsc::channel(", "unbounded(", "unbounded_channel("];

/// One counted call at a 1-based line in a repo-relative file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub line: usize,
}

/// Run the ratchet and return its exit status.
///
/// Takes the CLI arguments after `channels`; exits 2 on any argument, 1 when
/// the count is above the baseline, and 0 at or below it.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask channels");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("channels: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    match check(root) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("channels: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Read the baseline, scan the tracked tree, and report to stdout.
///
/// Returns the exit code chosen by `report`; fails when the baseline or a
/// tracked file cannot be read.
fn check(root: &Path) -> io::Result<ExitCode> {
    let baseline = read_baseline(&root.join(BASELINE_FILE))?;
    let files = tracked_files(root)?;
    let hits = scan(root, &files)?;
    let mut out = io::stdout();
    report(&hits, baseline, &mut out)
}

/// Print every hit, then the verdict.
///
/// Returns exit 1 above `baseline`; at or below it returns exit 0, and a count
/// below prints `lower the baseline to <n>`. Fails when writing `out` fails.
pub fn report(hits: &[Hit], baseline: usize, out: &mut dyn Write) -> io::Result<ExitCode> {
    for hit in hits {
        writeln!(out, "{}:{}", hit.path, hit.line)?;
    }
    let count = hits.len();
    match count.cmp(&baseline) {
        Ordering::Greater => {
            writeln!(
                out,
                "channels: {count} unbounded channels, baseline {baseline}"
            )?;
            Ok(ExitCode::FAILURE)
        }
        Ordering::Less => {
            writeln!(out, "lower the baseline to {count}")?;
            Ok(ExitCode::SUCCESS)
        }
        Ordering::Equal => {
            writeln!(
                out,
                "channels: {count} unbounded channels, baseline {baseline}"
            )?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Read the first non-comment line of a baseline file as a count.
///
/// Fails when the file cannot be read or holds no number.
fn read_baseline(path: &Path) -> io::Result<usize> {
    let text = fs::read_to_string(path)?;
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'));
    let Some(number) = first else {
        return Err(io::Error::other(format!(
            "{}: no baseline number",
            path.display()
        )));
    };
    number
        .parse()
        .map_err(|_| io::Error::other(format!("{}: not a number: {number}", path.display())))
}

/// List tracked `.rs` files, excluding `vendor/` and `target/`.
///
/// Fails when git cannot run or prints non-UTF-8 names.
fn tracked_files(root: &Path) -> io::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("git ls-files failed"));
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::other("git ls-files printed non-UTF-8 names"))?;
    Ok(listing
        .split('\0')
        .filter(|rel| rel.ends_with(".rs"))
        .filter(|rel| !is_excluded(rel))
        .map(str::to_string)
        .collect())
}

/// True when `rel` lives in a vendor or build-output folder.
fn is_excluded(rel: &str) -> bool {
    rel.split('/')
        .any(|part| part == "vendor" || part == "target")
}

/// True when `rel` names test code by file name or a `tests/` folder.
fn is_test_path(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = parts.last().copied().unwrap_or(rel);
    name == "tests.rs" || name.ends_with("_tests.rs") || parts.contains(&"tests")
}

/// Scan `files` (repo-relative under `root`) and return each counted call.
///
/// Test files and the items that `#[cfg(test)]` applies to are skipped.
/// Fails when a listed file cannot be read as UTF-8.
pub fn scan(root: &Path, files: &[String]) -> io::Result<Vec<Hit>> {
    let mut hits = Vec::new();
    for rel in files {
        if is_excluded(rel) || is_test_path(rel) {
            continue;
        }
        let text = fs::read_to_string(root.join(rel))?;
        let source = skip::scanned(&text);
        for (index, line) in source.code.iter().enumerate() {
            if source.is_test[index] {
                continue;
            }
            for _ in 0..counted(line) {
                hits.push(Hit {
                    path: rel.clone(),
                    line: index + 1,
                });
            }
        }
    }
    Ok(hits)
}

/// Count unbounded channel constructions on one source line.
fn counted(line: &str) -> usize {
    PATTERNS
        .iter()
        .map(|pattern| occurrences(line, pattern))
        .sum()
}

/// Count `pattern` in `line` where it starts an identifier, not a longer name.
fn occurrences(line: &str, pattern: &str) -> usize {
    line.match_indices(pattern)
        .filter(|(index, _)| {
            line[..*index]
                .chars()
                .next_back()
                .is_none_or(|c| !is_name_char(c))
        })
        .count()
}

/// True when `c` may continue a Rust identifier.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
