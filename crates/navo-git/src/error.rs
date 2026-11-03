use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::ObjectId;

#[derive(Debug, Clone)]
pub enum GitError {
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    NotARepository {
        path: PathBuf,
    },
    InvalidRepositoryFile {
        path: PathBuf,
        detail: String,
    },
    InvalidConfig {
        path: PathBuf,
        detail: String,
    },
    UnsupportedObjectFormat {
        value: String,
    },
    UnsupportedRefStorage {
        value: String,
    },
    Unsupported {
        operation: String,
    },
    GitNotFound,
    GitTooOld {
        version: String,
    },
    Command {
        args: String,
        status: Option<i32>,
        stderr: String,
    },
    UnsafeDirectory {
        path: PathBuf,
    },
    /// The guard is blocked (a config file could not be read or parsed); no background call ran.
    NotTrusted {
        path: PathBuf,
    },
    /// A filter driver other than the standard git-lfs one is in use, so the built-in engine
    /// would have to start a program. The Git command line neutralises it.
    FilterRefused {
        path: PathBuf,
        filter: String,
    },
    /// The repository config names a program (fsmonitor) the built-in engine must not start.
    ProgramConfigured {
        path: PathBuf,
    },
    /// The repository config names a program, so the background fetch was not run.
    Guarded {
        path: PathBuf,
    },
    NoUpstream,
    DetachedHead,
    /// The branch has no commit yet, so there is nothing to push.
    NoCommit,
    /// The upstream remote starts with `-` and could pass for a git option.
    UnsafeRemote {
        name: String,
    },
    /// The remote's fetch refspecs are not the single default, so a pull could write tags or
    /// local branches; nothing ran.
    UnsupportedRefspec,
    /// The upstream branch no longer exists on the remote while its tracking ref does; a push
    /// would recreate it, so nothing was sent.
    RemoteBranchGone,
    /// The upstream moved past a commit HEAD does not have; Pull stopped without a change.
    NotFastForward,
    /// Local changes or untracked files would be overwritten; Pull stopped without a change.
    LocalChanges,
    /// The remote refused the update (it moved, or a remote hook declined); nothing was forced.
    Rejected,
    /// The remote asked for credentials and no prompt may open.
    AuthFailed,
    /// The repository config changed after the user confirmed; nothing ran.
    GuardChanged,
    /// The branch, its upstream or HEAD changed after the user confirmed; nothing ran.
    TargetChanged,
    /// Pull exited 0 but HEAD does not hold the upstream, a stash was added or conflicts remain;
    /// the working tree may have changed.
    PullUnfinished,
    /// Pull was killed while Git ran; `lock` is a left `index.lock`, `moved` a changed HEAD.
    Interrupted {
        lock: bool,
        moved: bool,
    },
    Timeout,
    Cancelled,
    UnrepresentablePath,
    Protocol {
        detail: String,
    },
    SessionBusy,
    SessionClosed,
    MissingObject {
        id: ObjectId,
    },
    DuplicateEngine {
        id: String,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, kind } => write!(formatter, "{}: {kind:?}", path.display()),
            Self::NotARepository { path } => {
                write!(formatter, "not a Git repository: {}", path.display())
            }
            Self::InvalidRepositoryFile { path, detail } => {
                write!(formatter, "invalid Git file {}: {detail}", path.display())
            }
            Self::InvalidConfig { path, detail } => {
                write!(formatter, "invalid Git config {}: {detail}", path.display())
            }
            Self::UnsupportedObjectFormat { value } => {
                write!(formatter, "unsupported Git object format: {value}")
            }
            Self::UnsupportedRefStorage { value } => {
                write!(formatter, "unsupported Git ref storage: {value}")
            }
            Self::Unsupported { operation } => {
                write!(formatter, "the Git engine does not support {operation}")
            }
            Self::GitTooOld { version } => {
                write!(formatter, "git {version} is older than 2.36")
            }
            Self::Command {
                args,
                status,
                stderr,
            } => write!(formatter, "git {args} exited with {status:?}: {stderr}"),
            Self::UnsafeDirectory { path }
            | Self::NotTrusted { path }
            | Self::FilterRefused { path, .. }
            | Self::ProgramConfigured { path }
            | Self::Guarded { path } => write_refusal(formatter, self, path),
            Self::Interrupted { lock, moved } => write!(
                formatter,
                "pull was stopped while git ran (index.lock left: {lock}, HEAD moved: {moved})"
            ),
            Self::UnsafeRemote { name } => write!(formatter, "remote {name:?} starts with a dash"),
            Self::Protocol { detail } => write!(formatter, "unexpected git output: {detail}"),
            Self::MissingObject { id } => write!(formatter, "git object is missing: {id:?}"),
            Self::DuplicateEngine { id } => {
                write!(
                    formatter,
                    "a Git engine with id {id:?} is already registered"
                )
            }
            plain => formatter.write_str(plain_text(plain)),
        }
    }
}

fn plain_text(error: &GitError) -> &'static str {
    match error {
        GitError::GitNotFound => "git was not found on PATH",
        GitError::NoUpstream => "the branch has no upstream",
        GitError::DetachedHead => "HEAD is detached, so there is no upstream",
        GitError::NoCommit => "the branch has no commit yet",
        GitError::NotFastForward => "pull needs a merge",
        GitError::LocalChanges => "local changes would be overwritten by pull",
        GitError::Rejected => "the remote rejected the push",
        GitError::AuthFailed => "the remote needs credentials",
        GitError::GuardChanged => "the repository config changed since the confirmation",
        GitError::TargetChanged => {
            "the branch, its upstream or HEAD changed since the confirmation"
        }
        GitError::UnsupportedRefspec => {
            "the remote fetches more than its branches, so pull did not run"
        }
        GitError::RemoteBranchGone => "the branch is gone from the remote, so push did not run",
        GitError::PullUnfinished => "pull ended without reaching the upstream",
        GitError::Timeout => "git did not answer before the deadline",
        GitError::Cancelled => "the git call was cancelled",
        GitError::UnrepresentablePath => "path is not representable",
        GitError::SessionBusy => "the git cat-file session is busy",
        GitError::SessionClosed => "the git cat-file session closed",
        _ => "git failed",
    }
}

fn write_refusal(formatter: &mut fmt::Formatter<'_>, error: &GitError, path: &Path) -> fmt::Result {
    let reason = match error {
        GitError::FilterRefused { filter, .. } => {
            return write!(
                formatter,
                "the Git filter {filter:?} would start a program, so the built-in engine did not run: {}",
                path.display()
            );
        }
        GitError::NotTrusted { .. } => {
            "its config could not be read safely, so git was not started"
        }
        GitError::ProgramConfigured { .. } => {
            "its config names a program (fsmonitor), so the built-in engine did not run"
        }
        GitError::Guarded { .. } => "its config names a program, so fetch was not run",
        _ => "git refuses to use a directory owned by someone else",
    };
    write!(formatter, "{reason}: {}", path.display())
}

impl Error for GitError {}
