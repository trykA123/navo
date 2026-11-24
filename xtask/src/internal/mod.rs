//! Guard that no internal material (plans, design, testing, spikes, docs other
//! than the user guide) is tracked in the public repo. Runs as `cargo xtask internal`
//! and as a step of the gate and `check`.

#[cfg(test)]
mod tests;

use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};

/// Folders whose tracked files belong to the private navo-internal repo.
const INTERNAL_DIRS: [&str; 5] = ["plans/", "design/", "testing/", "spikes/", "docs/"];
/// The one `docs/` file the app compiles in, so it stays public.
const USER_GUIDE: &str = "docs/user-guide.md";

/// Return the sorted tracked paths that belong to the private repo.
///
/// `tracked` holds repo-relative paths with `/` separators; a path is internal
/// when it lies under one of the internal folders and is not the user guide.
pub fn check(tracked: &[String]) -> Vec<String> {
    let mut hits: Vec<String> = tracked
        .iter()
        .filter(|path| path.as_str() != USER_GUIDE)
        .filter(|path| INTERNAL_DIRS.iter().any(|dir| path.starts_with(dir)))
        .cloned()
        .collect();
    hits.sort();
    hits
}

/// Reject arguments, list tracked files, and print every internal one.
///
/// Takes the CLI arguments after `internal`; exits 2 on any argument, 1 on a
/// hit or a git failure, and 0 when no internal file is tracked.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask internal");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("internal: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let tracked = match tracked_files(root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("internal: {error}");
            return ExitCode::FAILURE;
        }
    };
    let hits = check(&tracked);
    if hits.is_empty() {
        println!("internal: 0 internal files tracked");
        return ExitCode::SUCCESS;
    }
    for path in &hits {
        println!("internal: tracked internal file: {path}");
    }
    ExitCode::FAILURE
}

/// Return every tracked path under `root`, from `git ls-files -z`.
///
/// Fails when git cannot run, exits non-zero, or prints non-UTF-8 names.
fn tracked_files(root: &Path) -> io::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .map_err(|_| io::Error::other("git ls-files failed"))?;
    if !output.status.success() {
        return Err(io::Error::other("git ls-files failed"));
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::other("git ls-files printed non-UTF-8 names"))?;
    Ok(listing
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}
