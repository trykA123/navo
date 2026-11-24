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

/// Constructor names that build an unbounded channel.
const CALLS: [&str; 3] = ["unbounded_channel", "unbounded", "channel"];

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

/// True when `rel` names test code by file name, `tests/` or `*_tests/`.
fn is_test_path(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = parts.last().copied().unwrap_or(rel);
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || parts
            .iter()
            .any(|part| *part == "tests" || part.ends_with("_tests"))
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
        let masked = skip::masked(&text);
        let imported = Imported::new(&masked);
        for (index, line) in masked.lines().enumerate() {
            for _ in 0..counted(line, imported) {
                hits.push(Hit {
                    path: rel.clone(),
                    line: index + 1,
                });
            }
        }
    }
    Ok(hits)
}

/// Unbounded constructors imported by name in one file.
#[derive(Debug, Clone, Copy, Default)]
struct Imported {
    unbounded: bool,
    unbounded_channel: bool,
    channel: bool,
}

impl Imported {
    /// Read the `use ...` statements of `masked` for bare constructor imports.
    fn new(masked: &str) -> Self {
        let mut imported = Self::default();
        for statement in masked.split(';') {
            let mpsc = statement.contains("mpsc");
            if !statement.contains("use ")
                || !(mpsc || statement.contains("crossbeam") || statement.contains("flume"))
            {
                continue;
            }
            imported.unbounded |= has_name(statement, "unbounded");
            imported.unbounded_channel |= mpsc && has_name(statement, "unbounded_channel");
            // `std::sync::mpsc` and crossbeam's `channel()` are unbounded. Tokio's
            // and futures' `channel(n)` take a capacity, so they stay uncounted.
            let unbounded_channel =
                statement.contains("std::sync::mpsc") || statement.contains("crossbeam_channel");
            imported.channel |= unbounded_channel && has_name(statement, "channel");
        }
        imported
    }
}

/// Count unbounded channel constructions on one line of `masked` code.
fn counted(line: &str, imported: Imported) -> usize {
    let mut count = 0;
    for name in CALLS {
        let mut from = 0;
        while let Some(offset) = line[from..].find(name) {
            let at = from + offset;
            if is_channel_call(line, at, name, imported) {
                count += 1;
            }
            from = at + name.len();
        }
    }
    count
}

/// True when `name` at `at` is a channel constructor called with `(`, not a
/// longer name, a definition, or an unrelated function.
fn is_channel_call(line: &str, at: usize, name: &str, imported: Imported) -> bool {
    if line[..at].chars().next_back().is_some_and(is_name_char) {
        return false;
    }
    let after = turbofish_end(line, at + name.len()).unwrap_or(at + name.len());
    if !line[after..].starts_with("()") {
        return false;
    }
    if is_definition(line, at) || line[..at].trim_end().ends_with('.') {
        return false;
    }
    let before = &line[..at];
    let mpsc_qualified = before.strip_suffix("mpsc::").is_some_and(|path| {
        path.chars()
            .next_back()
            .is_none_or(|character| !is_name_char(character))
    });
    if mpsc_qualified {
        return true;
    }
    if before.ends_with("::") {
        return false;
    }
    match name {
        "unbounded" => imported.unbounded,
        "unbounded_channel" => imported.unbounded_channel,
        "channel" => imported.channel,
        _ => false,
    }
}

/// End of a `::<...>` turbofish starting at `start`, if one starts there.
fn turbofish_end(line: &str, start: usize) -> Option<usize> {
    line[start..].strip_prefix("::<")?;
    let bytes = line.as_bytes();
    let mut depth = 1usize;
    let mut at = start + 3;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' if bytes.get(at - 1) != Some(&b'-') => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// True when the word before `at` is a definition keyword.
fn is_definition(line: &str, at: usize) -> bool {
    let before = line[..at].trim_end();
    let word = before
        .rsplit(|character: char| !is_name_char(character))
        .next();
    matches!(
        word,
        Some("fn" | "struct" | "enum" | "trait" | "union" | "type" | "use" | "mod" | "const")
    )
}

/// True when `name` appears in `text` as a whole identifier.
fn has_name(text: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = text[from..].find(name) {
        let at = from + offset;
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        if before.is_none_or(|character| !is_name_char(character))
            && after.is_none_or(|character| !is_name_char(character))
        {
            return true;
        }
        from = at + name.len();
    }
    false
}

/// True when `c` may continue a Rust identifier.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
