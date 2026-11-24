//! Runs one allowlisted command-line tool for the F3 editor (formatters, linters): an absolute
//! exe with an argv list, never a shell, with a timeout and a cap on captured output.

use std::path::PathBuf;
use std::time::Duration;

mod guard;
mod process;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_tests;

/// How long a tool may run when the caller has no setting of its own.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Most bytes kept from each of stdout and stderr (16 MiB); the rest is dropped.
pub const OUTPUT_CAP: usize = 16 * 1024 * 1024;

/// One tool run: the absolute exe, its arguments, bytes for stdin and the file being edited.
#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub exe: PathBuf,
    pub argv: Vec<String>,
    pub stdin: Vec<u8>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub edited_file: PathBuf,
}

/// What a finished tool returned; `truncated` is true when either stream passed the cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
}

/// Why a tool did not run or did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    NotAbsolute,
    NotFound,
    RepoLocal,
    Timeout,
    Spawn(String),
    Shell,
    FolderUnresolved,
}

/// Checks and runs the tool in `req`, keeping at most [`OUTPUT_CAP`] bytes per stream.
/// Fails before spawning when the exe is relative, missing, a shell or script, inside the
/// edited file's repository, or when that file's folder cannot be resolved (`FolderUnresolved`);
/// fails with `Timeout` after killing a tool that ran past `req.timeout`.
pub fn run_tool(req: &ToolRequest) -> Result<ToolOutput, ToolError> {
    run_tool_capped(req, OUTPUT_CAP)
}

/// [`run_tool`] with a chosen per-stream cap in bytes, so tests need not write 16 MiB.
pub(crate) fn run_tool_capped(req: &ToolRequest, cap: usize) -> Result<ToolOutput, ToolError> {
    let exe = guard::checked_exe(&req.exe, &req.edited_file)?;
    process::run(&exe, req, cap)
}
