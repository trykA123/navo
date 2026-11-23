//! Guard that every plan file is linked from `plans/README.md`.
//! Runs as `cargo xtask plans` and as a step of the gate and `check`.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};

const PLANS_DIR: &str = "plans";
const README: &str = "README.md";
const ALLOW_FILE: &str = "xtask/plans-allow.txt";

/// Problems found by comparing the plan files with the README links.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Number of plan files the README links.
    pub linked: usize,
    /// Plan files that neither the README nor the allow file lists.
    pub unlinked: Vec<String>,
    /// README links that point at no plan file.
    pub missing: Vec<String>,
    /// Allow entries that the README now links.
    pub stale_allow: Vec<String>,
}

/// Compare plan file names with README link targets and allow entries.
///
/// The two lists hold names relative to `plans/`; `allow` holds accepted
/// unlinked names. Returns the linked count and every problem, without disk.
pub fn check(plan_files: &[String], links: &[String], allow: &[String]) -> Report {
    let plans: BTreeSet<&str> = plan_files
        .iter()
        .map(String::as_str)
        .filter(|name| *name != README)
        .collect();
    let targets: BTreeSet<&str> = links
        .iter()
        .filter_map(|link| normalize(link))
        .filter(|name| *name != README)
        .collect();
    let allowed: BTreeSet<&str> = allow.iter().map(String::as_str).collect();
    let linked: BTreeSet<&str> = plans.intersection(&targets).copied().collect();
    let mut report = Report {
        linked: linked.len(),
        ..Report::default()
    };
    for name in plan_files {
        if name == README || linked.contains(name.as_str()) || allowed.contains(name.as_str()) {
            continue;
        }
        report.unlinked.push(name.clone());
    }
    for name in &targets {
        if !plans.contains(*name) {
            report.missing.push((*name).to_string());
        }
    }
    for name in allow {
        if linked.contains(name.as_str()) {
            report.stale_allow.push(name.clone());
        }
    }
    report
}

/// Reject arguments, read the plans folder, and print every link problem.
///
/// Takes the CLI arguments after `plans`; exits 2 on any argument, 1 on a
/// problem or read error, and 0 when every plan file is linked or allowed.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask plans");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("plans: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let plan_files = match read_plan_files(root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("plans: {error}");
            return ExitCode::FAILURE;
        }
    };
    let links = match readme_links(root) {
        Ok(links) => links,
        Err(error) => {
            eprintln!("plans: {error}");
            return ExitCode::FAILURE;
        }
    };
    let allow = match load_allow(root) {
        Ok(allow) => allow,
        Err(error) => {
            eprintln!("plans: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = check(&plan_files, &links, &allow);
    if report.unlinked.is_empty() && report.missing.is_empty() && report.stale_allow.is_empty() {
        println!("plans: {} files linked", report.linked);
        return ExitCode::SUCCESS;
    }
    for name in &report.unlinked {
        println!("plans: not linked from plans/README.md: {name}");
    }
    for name in &report.missing {
        println!("plans: README links a missing file: {name}");
    }
    for name in &report.stale_allow {
        println!("plans: remove from plans-allow.txt: {name}");
    }
    ExitCode::FAILURE
}

/// Return the tracked top-level `.md` names in `plans/`, without `README.md`,
/// sorted. Files the owner drops in the folder untracked are ignored.
fn read_plan_files(root: &Path) -> io::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", PLANS_DIR])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("git ls-files failed"));
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::other("git ls-files printed non-UTF-8 names"))?;
    let mut files = Vec::new();
    for rel in listing.split('\0').filter(|name| !name.is_empty()) {
        let Some(name) = rel.strip_prefix("plans/") else {
            continue;
        };
        if name.contains('/') || name == README || !name.ends_with(".md") {
            continue;
        }
        files.push(name.to_string());
    }
    files.sort();
    Ok(files)
}

/// Return the README text's local `.md` link targets, with anchors removed.
fn readme_links(root: &Path) -> io::Result<Vec<String>> {
    let path = root.join(PLANS_DIR).join(README);
    let text = fs::read_to_string(&path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    Ok(link_targets(&text))
}

/// Return every Markdown link target in `text` that names a local `.md` file.
fn link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for (index, _) in text.match_indices("](") {
        let rest = &text[index + 2..];
        let Some(end) = rest.find(')') else {
            continue;
        };
        if let Some(target) = normalize(&rest[..end]) {
            targets.push(target.to_string());
        }
    }
    targets
}

/// Return the `plans/`-relative file name in a link, or `None` for a link
/// that is external, absolute, an anchor, or not Markdown.
fn normalize(target: &str) -> Option<&str> {
    let path = target.trim().split('#').next().unwrap_or_default();
    let path = path.strip_prefix("./").unwrap_or(path);
    if path.is_empty() || path.contains("://") || path.starts_with('/') || !path.ends_with(".md") {
        return None;
    }
    Some(path)
}

/// Read the allow file and return its plan names.
///
/// Blank lines and lines starting with `#` are skipped.
fn load_allow(root: &Path) -> io::Result<Vec<String>> {
    let path = root.join(ALLOW_FILE);
    let text = fs::read_to_string(&path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}
