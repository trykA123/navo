//! Refusal checks for an editor tool exe: it must be absolute, exist, not be a shell and not
//! live inside the edited file's repository, where a cloned repo could plant it.

use std::path::{Path, PathBuf};

use super::ToolError;

/// File stems (lower case) of shells and script hosts; running one would let argv become a
/// command line. Trust model: the exe and argv come only from the user's own `navo.toml`, never
/// from a repo, so this list stops a mistaken setting; it is not a sandbox.
const SHELLS: [&str; 18] = [
    "cmd",
    "powershell",
    "pwsh",
    "sh",
    "bash",
    "zsh",
    "fish",
    "wsl",
    "env",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "busybox",
    "cscript",
    "wscript",
    "mshta",
    "conhost",
];

/// Script extensions (lower case) refused on every platform; Windows runs .bat and .cmd
/// through `cmd.exe /c`, which would parse argv as a command line.
const SCRIPT_EXTENSIONS: [&str; 4] = ["bat", "cmd", "ps1", "vbs"];

/// Returns the canonical exe path to spawn, or the reason it is refused.
pub(super) fn checked_exe(exe: &Path, edited_file: &Path) -> Result<PathBuf, ToolError> {
    if !exe.is_absolute() {
        return Err(ToolError::NotAbsolute);
    }
    let canonical = std::fs::canonicalize(exe).map_err(|_| ToolError::NotFound)?;
    if !canonical.is_file() {
        return Err(ToolError::NotFound);
    }
    let windows = cfg!(windows);
    let is_script = |path: &Path| is_shell(path) || !is_allowed_extension(path, windows);
    if is_script(exe) || is_script(&canonical) {
        return Err(ToolError::Shell);
    }
    if canonical.starts_with(repo_root(edited_file)?) {
        return Err(ToolError::RepoLocal);
    }
    Ok(canonical)
}

/// True when the path's file stem names a shell, ignoring case.
fn is_shell(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| SHELLS.contains(&stem.to_ascii_lowercase().as_str()))
}

/// True when the exe's extension may run directly: on Windows only `exe` and `com` (any case);
/// elsewhere anything but a [`SCRIPT_EXTENSIONS`] entry, including no extension.
pub(super) fn is_allowed_extension(path: &Path, windows: bool) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("exe" | "com") => true,
        Some(_) if windows => false,
        Some(other) => !SCRIPT_EXTENSIONS.contains(&other),
        None => !windows,
    }
}

/// The nearest canonical ancestor of the edited file that holds `.git`, else the file's folder.
/// Fails with `FolderUnresolved` when the folder cannot be canonicalised: a raw path would not
/// match the canonical exe, so the repo check would pass without checking anything.
fn repo_root(edited_file: &Path) -> Result<PathBuf, ToolError> {
    let folder = edited_file.parent().unwrap_or(edited_file);
    let folder = std::fs::canonicalize(folder).map_err(|_| ToolError::FolderUnresolved)?;
    Ok(folder
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(&folder)
        .to_path_buf())
}
