mod dirs;
mod operation;
mod parse;

pub use dirs::DirMap;
pub(crate) use dirs::Severity;

pub use parse::parse_porcelain_v2;

pub(crate) use operation::{fetched_ms, operation};

use crate::cli::Cancel;
use crate::trust::Guard;
use crate::{GitError, ObjectId};

/// Reads the worktree status of one repository; an engine never runs a program the guard blocks.
pub trait GitStatus {
    /// Fails with `NotTrusted` when `guard` is blocked, `Timeout` or `Cancelled` when cut short.
    fn status(&self, guard: &Guard, cancel: &Cancel) -> Result<RepoStatus, GitError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached(ObjectId),
    Unborn(String),
}

/// Remote-tracking branch and the commit counts against it, as of the last fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub name: String,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    pub head: Head,
    pub upstream: Option<Upstream>,
    pub changed: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub operation: Option<Operation>,
    pub dirs: DirMap,
    /// The output was cut at the size cap, so the counts are lower bounds.
    pub capped: bool,
    pub fetched_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    TimedOut,
    NoGit,
    NotTrusted,
    TooLarge,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    Conflict,
    Modified,
    Diverged,
    Ahead,
    Behind,
    Clean,
    Unknown(Reason),
}

/// Picks the single badge for a status, highest rung first.
#[must_use]
pub fn badge(status: &RepoStatus) -> Badge {
    let busy = matches!(
        status.operation,
        Some(Operation::Merge | Operation::Rebase | Operation::CherryPick | Operation::Revert)
    );
    if status.conflicted > 0 || busy {
        return Badge::Conflict;
    }
    if status.changed.saturating_add(status.untracked) > 0 {
        return Badge::Modified;
    }
    let (ahead, behind) = status
        .upstream
        .as_ref()
        .map_or((0, 0), |upstream| (upstream.ahead, upstream.behind));
    match (ahead > 0, behind > 0) {
        (true, true) => Badge::Diverged,
        (true, false) => Badge::Ahead,
        (false, true) => Badge::Behind,
        (false, false) => Badge::Clean,
    }
}

impl RepoStatus {
    /// Badge of a folder, given its path relative to the repository root with
    /// `/` separators. The empty path is the root. Subfolders are unknown when
    /// the status was capped or the folder map overflowed.
    #[must_use]
    pub fn dir_badge(&self, dir: &str) -> Badge {
        if dir.is_empty() {
            return badge(self);
        }
        if self.capped || self.dirs.overflowed() {
            return Badge::Unknown(Reason::TooLarge);
        }
        self.dirs.badge(dir)
    }
}
