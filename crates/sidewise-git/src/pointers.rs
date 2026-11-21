//! Checks a repository's pointer files before gix opens it, so a hostile repository cannot
//! make the open reach a network share (an SMB path leaks NTLM). Pure string checks, local reads.
//! Includes are not followed; the open blocks them.

use std::collections::HashSet;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use crate::config_scan::core_worktrees;
use crate::repo::GitProblem;

/// Most bytes read from one pointer file (64 KiB); a larger file is refused.
const MAX_POINTER_BYTES: u64 = 64 * 1024;
/// Most bytes read from one config file (1 MiB); a larger file is refused.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
/// Most object folders followed through `alternates` files.
const MAX_OBJECT_DIRS: usize = 16;

/// Refuses `path` with `Unsupported` when its `.git` file, its `commondir` or `gitdir` file,
/// a `core.worktree` value in `config` or `config.worktree`, or an `objects/info/alternates`
/// file (followed through local alternates) names a network path, a quoted path, or a file is
/// over its size cap; `Broken` when one cannot be read.
pub fn check(path: &Path) -> Result<(), GitProblem> {
    let git_dir = git_dir(path)?;
    let common = match read_pointer(&git_dir.join("commondir"), MAX_POINTER_BYTES)? {
        Some(text) => {
            let target = first_line(&text);
            refuse_network(target)?;
            // gix reads `gitdir` as the work tree path and checks its owner.
            if let Some(gitdir) = read_pointer(&git_dir.join("gitdir"), MAX_POINTER_BYTES)? {
                refuse_network(first_line(&gitdir))?;
                refuse_network(gitdir.trim())?;
            }
            git_dir.join(target)
        }
        None => git_dir.clone(),
    };
    for config in [
        git_dir.join("config"),
        common.join("config"),
        git_dir.join("config.worktree"),
    ] {
        check_config(&config)?;
    }
    check_alternates(vec![git_dir.join("objects"), common.join("objects")])
}

/// Refuses a `core.worktree` value in the config file at `config` that names a network path,
/// since gix checks that folder and its owner. An absent file passes.
fn check_config(config: &Path) -> Result<(), GitProblem> {
    let Some(text) = read_pointer(config, MAX_CONFIG_BYTES)? else {
        return Ok(());
    };
    for worktree in core_worktrees(&text)? {
        refuse_network(&worktree)?;
    }
    Ok(())
}

/// The Git folder `path` names: the target of a `.git` file (checked first), `path/.git`, or
/// `path` itself for a bare repository or a `.git` folder.
fn git_dir(path: &Path) -> Result<PathBuf, GitProblem> {
    let dot_git = path.join(".git");
    let pointer = if path.is_file() {
        path.to_path_buf()
    } else if dot_git.is_file() {
        dot_git
    } else if dot_git.is_dir() {
        return Ok(dot_git);
    } else {
        return Ok(path.to_path_buf());
    };
    let text = read_pointer(&pointer, MAX_POINTER_BYTES)?.unwrap_or_default();
    let Some(target) = first_line(&text).strip_prefix("gitdir:") else {
        // Not a pointer gix can follow; the open itself reports it.
        return Ok(pointer);
    };
    let target = target.trim();
    refuse_network(target)?;
    let base = pointer.parent().unwrap_or(path);
    Ok(base.join(target))
}

/// Walks `objects/info/alternates` files from `start`, refusing any network or quoted line.
/// Relative lines are followed from both the folder that names them (Git) and the first
/// folder (gix). More than `MAX_OBJECT_DIRS` folders is `Unsupported`.
fn check_alternates(start: Vec<PathBuf>) -> Result<(), GitProblem> {
    let origin = start.first().cloned().unwrap_or_default();
    let mut queue = start;
    let mut seen = HashSet::new();
    while let Some(dir) = queue.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        if seen.len() > MAX_OBJECT_DIRS {
            return Err(GitProblem::Unsupported);
        }
        let Some(text) = read_pointer(&dir.join("info").join("alternates"), MAX_POINTER_BYTES)?
        else {
            continue;
        };
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('"') {
                return Err(GitProblem::Unsupported);
            }
            refuse_network(line)?;
            queue.push(dir.join(line));
            queue.push(origin.join(line));
        }
    }
    Ok(())
}

/// `Unsupported` for a UNC (`\\host`, `//host`), device (`\\?\UNC\`, `\\.\`) or NT
/// (`\??\`) path, in either slash form.
pub(crate) fn refuse_network(text: &str) -> Result<(), GitProblem> {
    let back = text.replace('/', "\\");
    if back.starts_with("\\\\") || back.starts_with("\\??\\") {
        return Err(GitProblem::Unsupported);
    }
    Ok(())
}

/// The first line of `text`, trimmed.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

/// Reads the pointer or config file at `path` as lossy text: `None` when absent, `Unsupported`
/// over `limit` bytes, `Broken` for another read error.
fn read_pointer(path: &Path, limit: u64) -> Result<Option<String>, GitProblem> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(GitProblem::Broken),
    };
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| GitProblem::Broken)?;
    if bytes.len() as u64 > limit {
        return Err(GitProblem::Unsupported);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(test)]
#[path = "pointers_tests.rs"]
mod tests;
