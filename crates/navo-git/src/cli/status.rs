use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

use crate::GitError;
use crate::status::{GitStatus, RepoStatus, fetched_ms, operation, parse_porcelain_v2};
use crate::trust::Guard;

use super::{Cancel, CliGit, ProcessOptions, ProcessOutcome, ProcessOutput};

const STATUS_CAP: usize = 8 * 1024 * 1024;
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const UNMERGED_TIMEOUT: Duration = Duration::from_secs(5);

const STATUS_ARGS: [&str; 7] = [
    "status",
    "--porcelain=v2",
    "--branch",
    "-z",
    "--untracked-files=normal",
    "--ignore-submodules=dirty",
    "--no-renames",
];

/// Output cap and deadline of the status call (D100d: 8 MiB and 10 s).
#[derive(Debug, Clone, Copy)]
pub struct StatusLimits {
    pub stdout_cap: usize,
    pub timeout: Duration,
}

impl Default for StatusLimits {
    fn default() -> Self {
        Self {
            stdout_cap: STATUS_CAP,
            timeout: STATUS_TIMEOUT,
        }
    }
}

impl GitStatus for CliGit {
    fn status(&self, guard: &Guard, cancel: &Cancel) -> Result<RepoStatus, GitError> {
        self.status_with(guard, cancel, StatusLimits::default())
    }
}

impl CliGit {
    /// `status` with explicit limits. Runs nothing when `guard` is blocked.
    pub fn status_with(
        &self,
        guard: &Guard,
        cancel: &Cancel,
        limits: StatusLimits,
    ) -> Result<RepoStatus, GitError> {
        if guard.is_blocked() {
            return Err(GitError::NotTrusted {
                path: self.repo.clone(),
            });
        }
        let args = guarded_args(guard, &STATUS_ARGS);
        let options = ProcessOptions {
            stdout_cap: limits.stdout_cap,
            timeout: limits.timeout,
        };
        let output = self.run_cancellable(&args, cancel, &options)?;
        let capped = self.read_outcome(&args, output.outcome, &output)?;
        let mut status = parse_porcelain_v2(&output.stdout, capped)?;
        if capped && status.conflicted == 0 && self.has_unmerged(guard, cancel)? {
            status.conflicted = 1;
        }
        let git_dir = self.git_dir()?;
        status.operation = operation(&git_dir)?;
        status.fetched_ms = fetched_ms(&git_dir);
        Ok(status)
    }

    /// `ls-files -u -z` stops at the first record: one byte of output proves a conflict.
    fn has_unmerged(&self, guard: &Guard, cancel: &Cancel) -> Result<bool, GitError> {
        let args = guarded_args(guard, &["ls-files", "-u", "-z"]);
        let options = ProcessOptions {
            stdout_cap: 1,
            timeout: UNMERGED_TIMEOUT,
        };
        let output = self.run_cancellable(&args, cancel, &options)?;
        self.read_outcome(&args, output.outcome, &output)?;
        Ok(!output.stdout.is_empty())
    }

    /// Whether the output was cut at the cap; every other way of ending early is an error.
    pub(super) fn read_outcome(
        &self,
        args: &[OsString],
        outcome: ProcessOutcome,
        output: &ProcessOutput,
    ) -> Result<bool, GitError> {
        match outcome {
            ProcessOutcome::Cancelled => Err(GitError::Cancelled),
            ProcessOutcome::TimedOut => Err(GitError::Timeout),
            ProcessOutcome::Capped => Ok(true),
            ProcessOutcome::Done => match output.status {
                Some(status) if status.success() => Ok(false),
                Some(status) => Err(self.command_error(
                    args,
                    &Output {
                        status,
                        stdout: Vec::new(),
                        stderr: output.stderr.clone(),
                    },
                )),
                None => Err(GitError::SessionClosed),
            },
        }
    }

    pub(super) fn git_dir(&self) -> Result<PathBuf, GitError> {
        let stdout = &self.git_dir_output;
        let line = stdout.strip_suffix(b"\n").unwrap_or(stdout);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        path_of(line).ok_or_else(super::parse::protocol)
    }
}

fn guarded_args(guard: &Guard, tail: &[&str]) -> Vec<OsString> {
    let mut args = guard.args();
    args.extend(tail.iter().map(OsString::from));
    args
}

#[cfg(unix)]
fn path_of(bytes: &[u8]) -> Option<PathBuf> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    (!bytes.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(bytes)))
}

#[cfg(windows)]
fn path_of(bytes: &[u8]) -> Option<PathBuf> {
    let text = std::str::from_utf8(bytes).ok()?;
    (!text.is_empty()).then(|| PathBuf::from(text))
}
