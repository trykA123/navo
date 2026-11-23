//! Guard against build-machine paths and AI session links in tracked text files.
//! Runs as `cargo xtask paths` and as a step of the gate and `check`.

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};

const SESSION: &str = "claude.ai/code/session";
const ALLOW_FILE: &str = "xtask/paths-allow.txt";
const SELF_DIR: &str = "xtask/src/paths/";
const PERSONAL_FILE: &str = "info/navo-paths-deny";

/// One flagged line: its 1-based number and trimmed text.
pub struct Hit {
    pub line: usize,
    pub text: String,
}

/// One allow entry: a repo-relative path and a needle that must match a whole
/// path token. The characters around the needle must not be name characters,
/// so `/mnt/` does not allow `/mnt/real/x` but a trailing separator or
/// punctuation does.
pub struct Allow {
    pub path: String,
    pub needle: String,
}

/// Reject arguments and scan every tracked text file in the repository.
///
/// Takes the CLI arguments after `paths`; exits 2 on any argument, 1 on hits or
/// read errors, and 0 when every file is clean.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: cargo xtask paths");
        return ExitCode::from(2);
    }
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() else {
        eprintln!("paths: cannot determine the repository root");
        return ExitCode::FAILURE;
    };
    let allow = match load_allow(root) {
        Ok(allow) => allow,
        Err(()) => return ExitCode::FAILURE,
    };
    let personal = match personal_needles(root) {
        Ok(needles) => needles,
        Err(error) => {
            eprintln!("paths: {error}");
            return ExitCode::FAILURE;
        }
    };
    let files = match tracked_files(root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("paths: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (checked, hits) = match scan_repo(root, &files, &allow, &personal) {
        Ok(counts) => counts,
        Err(error) => {
            eprintln!("paths: {error}");
            return ExitCode::FAILURE;
        }
    };
    if hits > 0 {
        return ExitCode::FAILURE;
    }
    println!("paths: {checked} files checked");
    ExitCode::SUCCESS
}

/// Read the allow file and return its entries.
///
/// Prints `paths-allow.txt:<n>: bad line` and returns `Err` on a malformed line
/// or when the file cannot be read.
fn load_allow(root: &Path) -> Result<Vec<Allow>, ()> {
    let path = root.join(ALLOW_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("paths: {}: {error}", path.display());
            return Err(());
        }
    };
    let mut allow = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(" | ").map(str::trim).collect();
        match parts.as_slice() {
            [path, needle, reason]
                if !path.is_empty() && !needle.is_empty() && !reason.is_empty() =>
            {
                allow.push(Allow {
                    path: (*path).to_string(),
                    needle: (*needle).to_string(),
                });
            }
            _ => {
                eprintln!("paths-allow.txt:{}: bad line", index + 1);
                return Err(());
            }
        }
    }
    Ok(allow)
}

/// Read the personal deny list from `<git common dir>/info/navo-paths-deny`.
///
/// The file is local and untracked; a missing file means an empty list. Fails
/// when git cannot report the common dir or the file cannot be read.
fn personal_needles(root: &Path) -> io::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("git rev-parse --git-common-dir failed"));
    }
    let printed = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::other("git rev-parse printed a non-UTF-8 path"))?;
    let common = printed.trim();
    if common.is_empty() {
        return Err(io::Error::other("git rev-parse printed no common dir"));
    }
    load_personal_needles(&root.join(common).join(PERSONAL_FILE))
}

/// Load one literal personal needle per line from `path`; a missing file is empty.
fn load_personal_needles(path: &Path) -> io::Result<Vec<String>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// Return the repository-relative paths printed by `git ls-files`.
///
/// Fails when git cannot run, exits non-zero, or prints non-UTF-8 names.
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
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

/// Scan every listed file, print each hit as `path:line: text`, and return
/// `(files checked, hits)`. Personal needles print as
/// `paths: personal path: path:line` and apply to every text file. Fails when
/// a listed file cannot be read.
fn scan_repo(
    root: &Path,
    files: &[String],
    allow: &[Allow],
    personal: &[String],
) -> io::Result<(usize, usize)> {
    let mut checked = 0;
    let mut hits = 0;
    for rel in files {
        if rel.starts_with(SELF_DIR) || rel == ALLOW_FILE {
            continue;
        }
        let bytes = fs::read(root.join(rel))?;
        if is_binary(&bytes) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        checked += 1;
        for hit in scan(rel, text, allow) {
            println!("{rel}:{}: {}", hit.line, hit.text);
            hits += 1;
        }
        for line in personal_hits(text, personal) {
            println!("paths: personal path: {rel}:{line}");
            hits += 1;
        }
    }
    Ok((checked, hits))
}

/// True when the first 8 KiB hold a NUL byte, the usual binary-file marker.
fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

/// Return the generic hits for one file's text; `rel` is the repo-relative path.
fn scan(rel: &str, text: &str, allow: &[Allow]) -> Vec<Hit> {
    let paths_apply = !paths_are_ignored(rel);
    let mut hits = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let flagged = line.contains(SESSION)
            || (paths_apply
                && (has_mnt_path(line)
                    || has_home_path(line)
                    || has_windows_path(line)
                    || has_claude_dir(line)));
        if !flagged {
            continue;
        }
        let allowed = allow
            .iter()
            .any(|entry| entry.path == rel && allow_matches(line, entry));
        if !allowed {
            hits.push(Hit {
                line: index + 1,
                text: line.trim().to_string(),
            });
        }
    }
    hits
}

/// True when pattern B does not apply: Rust, snapshots, or fixture folders.
fn paths_are_ignored(rel: &str) -> bool {
    rel.ends_with(".rs")
        || rel.ends_with(".snap")
        || rel
            .split('/')
            .any(|part| part == "fixtures" || part == "snapshots")
}

/// Return the 1-based lines of `text` that contain any personal needle.
fn personal_hits(text: &str, needles: &[String]) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| needles.iter().any(|needle| line.contains(needle)))
        .map(|(index, _)| index + 1)
        .collect()
}

/// True when `line` holds `/mnt/` followed by at least one more character.
fn has_mnt_path(line: &str) -> bool {
    line.match_indices("/mnt/")
        .any(|(index, _)| index + 5 < line.len())
}

/// True when `line` holds a Linux `/home/<name>` or macOS `/Users/<name>` home.
fn has_home_path(line: &str) -> bool {
    has_user_home(line, "/home/") || has_user_home(line, "/Users/")
}

/// True when `line` holds `prefix` followed by a non-empty ASCII user name.
fn has_user_home(line: &str, prefix: &str) -> bool {
    line.match_indices(prefix)
        .any(|(index, _)| name_length(&line[index + prefix.len()..]) > 0)
}

/// True when `line` holds a drive letter, `:\Users\`, a name, and a separator.
/// The user folder accepts either slash and any case for `Users`.
fn has_windows_path(line: &str) -> bool {
    for (index, byte) in line.bytes().enumerate() {
        if !byte.is_ascii_alphabetic() {
            continue;
        }
        if index > 0 && line.as_bytes()[index - 1].is_ascii_alphanumeric() {
            continue;
        }
        let Some(rest) = line
            .get(index + 1..)
            .and_then(|rest| rest.strip_prefix(':'))
        else {
            continue;
        };
        let Some(tail) = rest.strip_prefix('\\').or_else(|| rest.strip_prefix('/')) else {
            continue;
        };
        let Some(users) = tail.get(..5) else {
            continue;
        };
        if !users.eq_ignore_ascii_case("Users") {
            continue;
        }
        let Some(tail) = tail[5..]
            .strip_prefix('\\')
            .or_else(|| tail[5..].strip_prefix('/'))
        else {
            continue;
        };
        let name = name_length(tail);
        let separator = tail.as_bytes().get(name);
        if name > 0 && matches!(separator, Some(b'\\' | b'/')) {
            return true;
        }
    }
    false
}

/// True when `line` holds a `~/.claude` handoff folder.
fn has_claude_dir(line: &str) -> bool {
    line.contains("~/.claude")
}

/// True when `entry` occurs in `line` as a whole path token: the characters
/// around it are not name characters, so `/mnt/` cannot allow `/mnt/real/x`.
fn allow_matches(line: &str, entry: &Allow) -> bool {
    let mut from = 0;
    while let Some(found) = line[from..].find(&entry.needle) {
        let start = from + found;
        let end = start + entry.needle.len();
        let before = line[..start].chars().next_back();
        let after = line[end..].chars().next();
        if before.is_none_or(|c| !is_name_char(c)) && after.is_none_or(|c| !is_name_char(c)) {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Count the leading `[A-Za-z0-9._-]` bytes of `text`.
fn name_length(text: &str) -> usize {
    text.bytes().take_while(|byte| is_name_byte(*byte)).count()
}

/// True when `byte` may appear in an ASCII path component.
fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// True when `c` may appear in an ASCII path component.
fn is_name_char(c: char) -> bool {
    c.is_ascii() && is_name_byte(c as u8)
}
