use std::path::Path;

use navo_git::trust::untrusted_reason;
use navo_git::{Cancel, CliGit, GitError, SyncKind, SyncSummary, SyncTarget};

/// What a git-cli Pull or Push confirmation names (D100e, D100g), bound to the config, branch,
/// upstream and HEAD the user saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confirmation {
    pub kind: SyncKind,
    pub branch: String,
    pub remote: String,
    pub remote_branch: String,
    /// Commits to send or receive, counted from the last fetch at `fetched_ms`.
    pub commits: u32,
    /// When the last fetch ended, in ms since the Unix epoch; `None` if never.
    pub fetched_ms: Option<i64>,
    /// Push only: the push creates the remote branch.
    pub creates: bool,
    /// The commit HEAD named, in full; `None` on an unborn branch.
    pub head: Option<String>,
    /// Repository config keys that can run a program during the operation.
    pub guarded: Vec<String>,
    target: SyncTarget,
}

/// Why a Pull or Push did not finish. Refusals (up to `NotTrusted`) ran no writing Git process.
/// `Interrupted` is a Pull stopped while Git ran: it may have left `index.lock`, moved HEAD or
/// written part of the working tree. `Unfinished` is a Pull that exited 0 without reaching the
/// upstream, or that left a stash or conflicts. A `Cancelled` or `TimedOut` Push may or may not
/// have updated the remote. `Failed` is Git's own error and may follow a moved HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncFailure {
    NotFastForward,
    LocalChanges,
    Rejected,
    AuthFailed,
    NoUpstream,
    /// The remote's name could pass for a Git option or break a config key.
    UnsafeRemote,
    /// The remote's fetch refspec is not the single default; Pull would write more than its branch.
    UnsupportedRefspec,
    /// Push only: the branch was deleted on the remote, and a push would recreate it.
    RemoteBranchGone,
    Detached,
    ConfigChanged,
    TargetChanged,
    Interrupted {
        lock: bool,
        moved: bool,
    },
    Unfinished,
    Cancelled,
    TimedOut,
    NoGit,
    NotTrusted,
    Failed,
}

/// Reads the confirmation and runs the confirmed operation; both block, so callers stay off the UI thread.
pub(crate) trait SyncPort: Send + Sync + 'static {
    fn confirm(&self, repo: &Path, kind: SyncKind) -> Result<Confirmation, SyncFailure>;
    fn run(
        &self,
        repo: &Path,
        confirmed: &Confirmation,
        cancel: &Cancel,
    ) -> Result<SyncSummary, SyncFailure>;
}

/// The Git command line.
#[derive(Debug)]
pub(crate) struct CliSync;

impl SyncPort for CliSync {
    fn confirm(&self, repo: &Path, kind: SyncKind) -> Result<Confirmation, SyncFailure> {
        let read = || {
            let git = CliGit::new(repo)?;
            let guard = git.guard()?;
            let preview = git.sync_preview(kind, &guard)?;
            let tracking = &preview.target.tracking;
            Ok(Confirmation {
                kind,
                branch: tracking.branch.clone(),
                remote: tracking.remote.clone(),
                remote_branch: tracking.remote_branch.clone(),
                commits: preview.commits,
                fetched_ms: preview.fetched_ms,
                creates: preview.creates,
                head: preview.target.head.clone(),
                guarded: preview.guarded,
                target: preview.target,
            })
        };
        read().map_err(|error: GitError| failure(&error))
    }

    fn run(
        &self,
        repo: &Path,
        confirmed: &Confirmation,
        cancel: &Cancel,
    ) -> Result<SyncSummary, SyncFailure> {
        let git = CliGit::new(repo).map_err(|error| failure(&error))?;
        let done = match confirmed.kind {
            SyncKind::Pull => git.pull(&confirmed.target, cancel),
            SyncKind::Push => git.push(&confirmed.target, cancel),
        };
        done.map_err(|error| failure(&error))
    }
}

fn failure(error: &GitError) -> SyncFailure {
    match error {
        GitError::NotFastForward => SyncFailure::NotFastForward,
        GitError::LocalChanges => SyncFailure::LocalChanges,
        GitError::Rejected => SyncFailure::Rejected,
        GitError::AuthFailed => SyncFailure::AuthFailed,
        GitError::NoUpstream => SyncFailure::NoUpstream,
        GitError::UnsafeRemote { .. } => SyncFailure::UnsafeRemote,
        GitError::UnsupportedRefspec => SyncFailure::UnsupportedRefspec,
        GitError::RemoteBranchGone => SyncFailure::RemoteBranchGone,
        GitError::DetachedHead => SyncFailure::Detached,
        GitError::GuardChanged => SyncFailure::ConfigChanged,
        GitError::TargetChanged => SyncFailure::TargetChanged,
        GitError::Interrupted { lock, moved } => SyncFailure::Interrupted {
            lock: *lock,
            moved: *moved,
        },
        GitError::PullUnfinished => SyncFailure::Unfinished,
        GitError::Cancelled => SyncFailure::Cancelled,
        GitError::Timeout => SyncFailure::TimedOut,
        GitError::GitNotFound | GitError::GitTooOld { .. } => SyncFailure::NoGit,
        other if untrusted_reason(other).is_some() => SyncFailure::NotTrusted,
        _ => SyncFailure::Failed,
    }
}

#[cfg(test)]
#[path = "pullpush_tests.rs"]
pub(super) mod tests;
