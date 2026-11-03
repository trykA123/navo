use std::ffi::OsString;
use std::fs;
use std::time::{Duration, Instant};

use crate::GitError;
use crate::status::fetched_ms;
use crate::trust::{Guard, NO_AUTO_MAINTENANCE, maintenance_off};

use super::CliGit;
use super::process_watch::Cancel;
use super::sync_check::{Ran, RemoteBinding, require_default_fetch};
use super::sync_read::{Tracking, fetched_oid, needs_credentials, pushed_flag};

/// The deadline of a user-started Pull or Push (D100f).
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(120);

const NO_PROMPT: [&str; 2] = ["-c", "credential.interactive=never"];

const FETCH: [&str; 9] = [
    "fetch",
    "--no-all",
    "--no-recurse-submodules",
    "--no-prune",
    "--no-tags",
    "--write-fetch-head",
    "--refmap=",
    NO_AUTO_MAINTENANCE,
    "--end-of-options",
];

/// Hooks off for calls that only move refs Navo checked; the user's hooks run with the merge.
const NO_HOOKS: [&str; 2] = ["-c", "core.hooksPath="];

const MERGE: [&str; 8] = [
    "-c",
    "submodule.recurse=false",
    "merge",
    "--ff-only",
    "--no-autostash",
    "--no-squash",
    "--no-stat",
    "--end-of-options",
];

const PUSH: [&str; 4] = [
    "push",
    "--porcelain",
    "--no-follow-tags",
    "--no-recurse-submodules",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    Pull,
    Push,
}

/// What the user confirmed: the repository config, the branch with its upstream, the remote's
/// addresses and fetch refspecs, HEAD, and the commit count shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTarget {
    pub guard: Guard,
    pub tracking: Tracking,
    pub remote: RemoteBinding,
    pub head: Option<String>,
    pub commits: u32,
}

/// What a Pull or Push confirmation names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPreview {
    pub target: SyncTarget,
    /// Commits to receive or send, counted from the remote-tracking ref of the last fetch.
    pub commits: u32,
    pub guarded: Vec<String>,
    /// Push only: no remote-tracking ref exists, so the push creates the remote branch.
    pub creates: bool,
    /// When the last fetch wrote `FETCH_HEAD`, in ms since the Unix epoch; `None` if never.
    pub fetched_ms: Option<i64>,
}

/// What a finished Pull or Push did; `commits == 0` means it was already up to date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSummary {
    pub kind: SyncKind,
    pub branch: String,
    pub remote: String,
    pub commits: u32,
    /// Pull only: HEAD moved, but the remote-tracking ref still names the old commit until the
    /// next fetch.
    pub tracking_stale: bool,
}

impl CliGit {
    /// The target, the commit count and `guard`'s keys a confirmation shows. Runs no network
    /// call and writes nothing.
    pub fn sync_preview(&self, kind: SyncKind, guard: &Guard) -> Result<SyncPreview, GitError> {
        let tracking = self.tracking()?;
        let head = self.commit_of("HEAD")?;
        let (commits, creates) = match kind {
            SyncKind::Pull => (self.behind(&tracking)?, false),
            SyncKind::Push => (
                self.unpushed(&tracking)?,
                self.commit_of(&tracking.tracking_ref)?.is_none(),
            ),
        };
        let remote = self.remote_binding(&tracking.remote)?;
        Ok(SyncPreview {
            commits,
            creates,
            guarded: guard.keys().into_iter().map(str::to_owned).collect(),
            fetched_ms: fetched_ms(&self.git_dir()?),
            target: SyncTarget {
                guard: guard.clone(),
                tracking,
                remote,
                head,
                commits,
            },
        })
    }

    /// Fetches the confirmed upstream branch into `FETCH_HEAD` only, checks `target` again, then
    /// fast-forwards HEAD to the fetched commit with the user's config and moves the
    /// remote-tracking ref after it; never autostash, squash, prune or tags. Refuses before the
    /// fetch and before the merge when `target` no longer holds.
    pub fn pull(&self, target: &SyncTarget, cancel: &Cancel) -> Result<SyncSummary, GitError> {
        self.pull_with(target, cancel, SYNC_TIMEOUT)
    }

    /// `pull` with one deadline for the fetch, the merge and the tracking ref together.
    pub fn pull_with(
        &self,
        target: &SyncTarget,
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<SyncSummary, GitError> {
        let started = Instant::now();
        self.confirmed(target)?;
        let tracking = &target.tracking;
        let stash = self.commit_of("refs/stash")?;
        let tracked = self.commit_of(&tracking.tracking_ref)?;
        let fetch = fetch_args(tracking)?;
        require_default_fetch(&target.remote, &tracking.remote)?;
        let output = match self.sync_run(&fetch, cancel, timeout)? {
            Ran::Exited(output) => output,
            Ran::Killed(_) => return Err(self.interrupted(target.head.as_deref())),
        };
        if !output.status.is_some_and(|status| status.success()) {
            if needs_credentials(&output.stderr) {
                return Err(GitError::AuthFailed);
            }
            return Err(self.failed(&fetch, &output));
        }
        let upstream = self.fetched_commit(tracking)?;
        self.confirmed(target)?;
        if let Some(head) = target.head.as_deref()
            && !self.is_ancestor(head, &upstream)?
        {
            return Err(GitError::NotFastForward);
        }
        let merge = merge_args(&upstream);
        let left = timeout.saturating_sub(started.elapsed());
        let output = match self.sync_run(&merge, cancel, left)? {
            Ran::Exited(output) => output,
            Ran::Killed(_) => return Err(self.interrupted(target.head.as_deref())),
        };
        let after = self.commit_of("HEAD")?;
        if !output.status.is_some_and(|status| status.success()) {
            if after != target.head {
                return Err(self.failed(&merge, &output));
            }
            return Err(self.pull_failure(&merge, &output, &upstream));
        }
        self.verify_pull(&upstream, stash.as_deref())?;
        let commits = self.count_between(target.head.as_deref(), after.as_deref())?;
        let mut done = summary(SyncKind::Pull, tracking, commits);
        let left = timeout.saturating_sub(started.elapsed());
        let moved = (tracked.as_deref(), upstream.as_str());
        done.tracking_stale = !self.move_tracking(&tracking.tracking_ref, moved, (cancel, left));
        Ok(done)
    }

    /// The one commit the fetch wrote to `FETCH_HEAD` for the upstream branch; refuses a file
    /// another fetch rewrote meanwhile.
    fn fetched_commit(&self, tracking: &Tracking) -> Result<String, GitError> {
        let path = self.git_dir()?.join("FETCH_HEAD");
        let text = fs::read_to_string(&path).map_err(|error| GitError::Io {
            path,
            kind: error.kind(),
        })?;
        let oid = fetched_oid(&text, &tracking.remote_branch).ok_or(GitError::TargetChanged)?;
        self.commit_of(oid)?.ok_or(GitError::TargetChanged)
    }

    /// Moves `tracking_ref` from `old` to `new`, as the fetch would have, only while it still
    /// holds `old`; hooks off, since the merge already ran them. True when it holds `new`.
    fn move_tracking(
        &self,
        tracking_ref: &str,
        (old, new): (Option<&str>, &str),
        (cancel, timeout): (&Cancel, Duration),
    ) -> bool {
        if old == Some(new) {
            return true;
        }
        let absent = "0".repeat(new.len());
        let mut args = base_args();
        args.extend(NO_HOOKS.map(OsString::from));
        let update = ["update-ref", "--no-deref", "-m", "pull: fast-forward"];
        args.extend(update.map(OsString::from));
        args.extend(["--end-of-options", tracking_ref, new].map(OsString::from));
        args.push(OsString::from(old.unwrap_or(&absent)));
        match self.sync_run(&args, cancel, timeout) {
            Ok(Ran::Exited(output)) => output.status.is_some_and(|status| status.success()),
            _ => false,
        }
    }

    /// `git push --porcelain <remote> <confirmed HEAD>:refs/heads/<upstream branch>`: only the
    /// confirmed commits, never forced. Refuses before any write when `target` no longer holds.
    pub fn push(&self, target: &SyncTarget, cancel: &Cancel) -> Result<SyncSummary, GitError> {
        self.push_with(target, cancel, SYNC_TIMEOUT)
    }

    /// `push` with an explicit deadline.
    pub fn push_with(
        &self,
        target: &SyncTarget,
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<SyncSummary, GitError> {
        self.confirmed(target)?;
        let tracking = &target.tracking;
        let Some(head) = target.head.as_deref() else {
            return Err(GitError::NoCommit);
        };
        let args = push_args(tracking, head)?;
        if self.unpushed(tracking)? != target.commits {
            return Err(GitError::TargetChanged);
        }
        self.require_remote_branch(target, cancel, timeout)?;
        let output = match self.sync_run(&args, cancel, timeout)? {
            Ran::Exited(output) => output,
            Ran::Killed(error) => return Err(error),
        };
        let flag = pushed_flag(&output.stdout, &tracking.remote_branch);
        if output.status.is_some_and(|status| status.success()) {
            let commits = if flag == Some('=') { 0 } else { target.commits };
            return Ok(summary(SyncKind::Push, tracking, commits));
        }
        if flag == Some('!') {
            return Err(GitError::Rejected);
        }
        if needs_credentials(&output.stderr) {
            return Err(GitError::AuthFailed);
        }
        Err(self.failed(&args, &output))
    }
}

/// The fetch line after `git -C <repo>`: the upstream branch alone, into `FETCH_HEAD` only. The
/// empty refmap writes no ref, so hooks stay off: a hook added while the fetch runs cannot run
/// before the second check. `fetch.pruneTags` would fetch every tag even with `--no-tags`, so
/// both of its keys are off; a remote name with `=` cannot form such a key.
pub(super) fn fetch_args(tracking: &Tracking) -> Result<Vec<OsString>, GitError> {
    if tracking.remote.contains('=') {
        return Err(GitError::UnsafeRemote {
            name: tracking.remote.clone(),
        });
    }
    let mut args = base_args();
    args.extend(NO_HOOKS.map(OsString::from));
    args.extend(["-c", "fetch.pruneTags=false", "-c"].map(OsString::from));
    args.push(OsString::from(format!(
        "remote.{}.pruneTags=false",
        tracking.remote
    )));
    args.extend(FETCH.iter().map(OsString::from));
    args.push(OsString::from(&tracking.remote));
    args.push(OsString::from(format!(
        "refs/heads/{}",
        tracking.remote_branch
    )));
    Ok(args)
}

/// The merge line after `git -C <repo>`: a fast-forward to the fetched commit only.
fn merge_args(upstream: &str) -> Vec<OsString> {
    let mut args = base_args();
    args.extend(MERGE.iter().map(OsString::from));
    args.push(OsString::from(upstream));
    args
}

/// The argument line after `git -C <repo>`: the remote and one refspec from the confirmed commit.
/// `remote.<name>.mirror` is off, since a mirror push rejects a refspec or deletes other branches.
pub(super) fn push_args(tracking: &Tracking, head: &str) -> Result<Vec<OsString>, GitError> {
    if tracking.remote.contains('=') {
        return Err(GitError::UnsafeRemote {
            name: tracking.remote.clone(),
        });
    }
    let mut args = base_args();
    args.push(OsString::from("-c"));
    args.push(OsString::from(format!(
        "remote.{}.mirror=false",
        tracking.remote
    )));
    args.extend(PUSH.iter().map(OsString::from));
    args.push(OsString::from(&tracking.remote));
    args.push(OsString::from(format!(
        "{head}:refs/heads/{}",
        tracking.remote_branch
    )));
    Ok(args)
}

pub(super) fn base_args() -> Vec<OsString> {
    let mut args: Vec<OsString> = NO_PROMPT.iter().map(OsString::from).collect();
    args.extend(maintenance_off());
    args
}

fn summary(kind: SyncKind, tracking: &Tracking, commits: u32) -> SyncSummary {
    SyncSummary {
        kind,
        branch: tracking.branch.clone(),
        remote: tracking.remote.clone(),
        commits,
        tracking_stale: false,
    }
}
