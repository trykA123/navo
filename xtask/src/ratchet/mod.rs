//! Ratchet command for lint exceptions and test sleeps.
//! It compares counts in tracked `.rs` files with the ceilings in
//! `xtask/ratchet.toml`; the counter is a text scan, not a parser.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

const CONFIG: &str = "xtask/ratchet.toml";

// Split so the text scan does not count the scanner itself.
const ALLOW: &str = concat!("#[", "allow(");
const EXPECT: &str = concat!("#[", "expect(");
const SLEEP: &str = concat!("sleep", "(");

#[derive(serde::Deserialize)]
struct RawConfig {
    lint_exceptions: RawLimit,
    test_sleeps: RawLimit,
}

#[derive(serde::Deserialize)]
struct RawLimit {
    max: usize,
}

/// Ceilings for the two counters, read from `xtask/ratchet.toml`.
struct Limits {
    lint: usize,
    sleeps: usize,
}

/// Per-file item counts and their total for one counter.
#[derive(Default)]
struct Counts {
    items: BTreeMap<String, usize>,
    total: usize,
}

impl Counts {
    /// Record `count` items found in `path`, ignoring zero counts.
    fn add(&mut self, path: String, count: usize) {
        if count > 0 {
            self.total += count;
            self.items.insert(path, count);
        }
    }

    /// Return the `limit` files with the most items, ties ordered by path.
    fn top(&self, limit: usize) -> Vec<(&str, usize)> {
        let mut files: Vec<(&str, usize)> = self
            .items
            .iter()
            .map(|(path, count)| (path.as_str(), *count))
            .collect();
        files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        files.truncate(limit);
        files
    }
}

/// Both counters after one walk over the tracked sources.
#[derive(Default)]
struct Scan {
    lint: Counts,
    sleeps: Counts,
}

/// One report: its text and whether any counter is above its ceiling.
struct Report {
    text: String,
    over: bool,
}

/// Run `cargo xtask ratchet` against the repository root.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask ratchet");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("ratchet: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let mut out = io::stdout();
    let mut err = io::stderr();
    run_in(root, &mut out, &mut err)
}

/// Scan `root`, compare with its config and write the report to `out`.
///
/// Returns 1 when a counter is above its ceiling, 0 otherwise, and 1 when
/// the tree or the config cannot be read.
fn run_in(root: &Path, out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    let limits = match limits(root) {
        Ok(limits) => limits,
        Err(error) => {
            let _ = writeln!(err, "ratchet: {error}");
            return ExitCode::FAILURE;
        }
    };
    let scan = match scan(root) {
        Ok(scan) => scan,
        Err(error) => {
            let _ = writeln!(err, "ratchet: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = report(&scan, &limits);
    if write!(out, "{}", report.text).is_err() {
        return ExitCode::FAILURE;
    }
    if report.over {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Read and parse the two ceilings from `xtask/ratchet.toml` under `root`.
fn limits(root: &Path) -> io::Result<Limits> {
    let path = root.join(CONFIG);
    let text = fs::read_to_string(&path)?;
    let raw: RawConfig = toml::from_str(&text)
        .map_err(|error| io::Error::other(format!("{}: {error}", path.display())))?;
    Ok(Limits {
        lint: raw.lint_exceptions.max,
        sleeps: raw.test_sleeps.max,
    })
}

/// Count lint exceptions and test sleeps across the tracked `.rs` files.
fn scan(root: &Path) -> io::Result<Scan> {
    let mut scan = Scan::default();
    for relative in tracked_rs(root)? {
        let text = fs::read_to_string(root.join(&relative))?;
        let (lint, sleeps) = count_source(&relative, &text);
        scan.lint.add(relative.clone(), lint);
        scan.sleeps.add(relative, sleeps);
    }
    Ok(scan)
}

/// List tracked `.rs` files, excluding `vendor/` and `target/`.
fn tracked_rs(root: &Path) -> io::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "*.rs"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "git ls-files failed: {}",
            message.trim()
        )));
    }
    let list = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::other("git ls-files returned non-UTF-8 paths"))?;
    Ok(list
        .split('\0')
        .filter(|path| !path.is_empty() && !excluded(path))
        .map(str::to_owned)
        .collect())
}

/// Say whether a path is under the top-level `vendor/` or `target/`.
fn excluded(path: &str) -> bool {
    path.starts_with("vendor/") || path.starts_with("target/")
}

/// Count lint attributes (non-test) and `sleep` calls (test) in one source.
fn count_source(relative: &str, text: &str) -> (usize, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let whole_file_is_test = is_test_path(relative);
    let test_from = if whole_file_is_test {
        None
    } else {
        test_module_line(&lines)
    };
    let mut lint = 0;
    let mut sleeps = 0;
    for (index, line) in lines.iter().enumerate() {
        if whole_file_is_test || test_from.is_some_and(|start| index >= start) {
            sleeps += line.matches(SLEEP).count();
        } else {
            lint += line.matches(ALLOW).count() + line.matches(EXPECT).count();
        }
    }
    (lint, sleeps)
}

/// Say whether a repo-relative path names test code by its file or directory.
fn is_test_path(relative: &str) -> bool {
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return part == "tests.rs" || part.ends_with("_tests.rs");
        }
        if part == "tests" {
            return true;
        }
    }
    false
}

/// Find the first `#[cfg(test)]` that introduces a module in `lines`.
fn test_module_line(lines: &[&str]) -> Option<usize> {
    const MARKER: &str = "#[cfg(test)]";
    for (index, line) in lines.iter().enumerate() {
        let Some(at) = line.find(MARKER) else {
            continue;
        };
        if starts_module(&line[at + MARKER.len()..]) {
            return Some(index);
        }
        let mut next = index + 1;
        while let Some(candidate) = lines.get(next) {
            let trimmed = candidate.trim();
            if trimmed.is_empty() || trimmed.starts_with("#[") || trimmed.starts_with("//") {
                next += 1;
                continue;
            }
            if starts_module(candidate) {
                return Some(index);
            }
            break;
        }
    }
    None
}

/// Say whether the text begins a module declaration (`mod name`).
fn starts_module(text: &str) -> bool {
    let mut rest = text.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
    }
    if let Some(after) = rest.strip_prefix('(') {
        let Some(end) = after.find(')') else {
            return false;
        };
        rest = after[end + 1..].trim_start();
    }
    let Some(after) = rest.strip_prefix("mod") else {
        return false;
    };
    after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || c == '{')
}

/// Build the report for both counters.
fn report(scan: &Scan, limits: &Limits) -> Report {
    let mut text = String::new();
    let mut over = append(&mut text, "lint exceptions", &scan.lint, limits.lint);
    over |= append(&mut text, "test sleeps", &scan.sleeps, limits.sleeps);
    Report { text, over }
}

/// Append one counter's total, its top files, or the lower-baseline hint.
/// Returns whether the counter is above its ceiling.
fn append(text: &mut String, label: &str, counts: &Counts, max: usize) -> bool {
    let _ = writeln!(text, "{label}: {} (max {max})", counts.total);
    if counts.total > max {
        for (path, count) in counts.top(10) {
            let _ = writeln!(text, "  {path}: {count}");
        }
        true
    } else {
        if counts.total < max {
            let _ = writeln!(text, "lower the baseline to {}", counts.total);
        }
        false
    }
}
