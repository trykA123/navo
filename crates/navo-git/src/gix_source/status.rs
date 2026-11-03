use std::sync::atomic::AtomicBool;

use gix::bstr::BString;
use gix::status::{Submodule, UntrackedFiles, tree_index::TrackRenames};
use gix::submodule::config::Ignore;

use super::{GixGit, gix_command};
use crate::GitError;
use crate::cli::{Cancel, StatusLimits};
use crate::status::{GitStatus, RepoStatus, fetched_ms, operation};
use crate::trust::Guard;

mod refs;
mod tally;
mod watch;

use tally::Tally;
use watch::Watch;

/// Config keys under which gix would run a program or trust a monitor; `status` refuses them.
const REFUSED_KEYS: [&str; 2] = ["core.fsmonitor", "core.usebuiltinfsmonitor"];

impl GitStatus for GixGit {
    fn status(&self, guard: &Guard, cancel: &Cancel) -> Result<RepoStatus, GitError> {
        self.status_with(guard, cancel, StatusLimits::default())
    }
}

impl GixGit {
    /// The D100e guard for this repository, read without running Git.
    pub fn guard(&self) -> Result<Guard, GitError> {
        let repo = self.open()?;
        Guard::read(repo.git_dir())
    }

    /// `status` with explicit limits. The output cap is estimated from the size
    /// `git status --porcelain=v2` would print. Runs nothing when the config could run a program.
    pub fn status_with(
        &self,
        guard: &Guard,
        cancel: &Cancel,
        limits: StatusLimits,
    ) -> Result<RepoStatus, GitError> {
        if refuses(guard) {
            return Err(self.not_trusted());
        }
        if cancel.is_cancelled() {
            return Err(GitError::Cancelled);
        }
        let repo = self.open()?;
        if has_filter_driver(&repo) {
            return Err(self.not_trusted());
        }
        let watch = Watch::start(cancel, limits.timeout).map_err(|error| GitError::Io {
            path: self.repo.clone(),
            kind: error.kind(),
        })?;
        let result = read_status(&repo, &watch, limits.stdout_cap);
        match watch.interruption() {
            Some(error) => Err(error),
            None => result,
        }
    }

    fn not_trusted(&self) -> GitError {
        GitError::NotTrusted {
            path: self.repo.clone(),
        }
    }
}

fn refuses(guard: &Guard) -> bool {
    guard.is_blocked()
        || guard
            .keys()
            .iter()
            .any(|key| key.starts_with("filter.") || REFUSED_KEYS.contains(key))
}

/// Any filter driver with a command, from any config level, since gix would run it.
fn has_filter_driver(repo: &gix::Repository) -> bool {
    let snapshot = repo.config_snapshot();
    let Some(mut sections) = snapshot.plumbing().sections_by_name("filter") else {
        return false;
    };
    sections.any(|section| {
        ["clean", "smudge", "process"]
            .iter()
            .any(|key| section.value(*key).is_some())
    })
}

fn read_status(repo: &gix::Repository, watch: &Watch, cap: usize) -> Result<RepoStatus, GitError> {
    let head = refs::head(repo)?;
    let upstream = refs::upstream(repo, &head, watch.stopped())?;
    let (tally, capped) = collect(repo, watch, cap)?;
    let mut counts = tally.counts();
    if capped && counts.conflicted == 0 && has_unmerged(repo)? {
        counts.conflicted = 1;
    }
    let git_dir = repo.git_dir();
    Ok(RepoStatus {
        head: head.head,
        upstream,
        changed: counts.changed,
        untracked: counts.untracked,
        conflicted: counts.conflicted,
        operation: operation(git_dir)?,
        dirs: counts.dirs,
        capped,
        fetched_ms: fetched_ms(git_dir),
    })
}

fn collect(repo: &gix::Repository, watch: &Watch, cap: usize) -> Result<(Tally, bool), GitError> {
    let items = status_iter(repo, watch.flag())?;
    let mut tally = Tally::default();
    for item in items {
        let item = match item {
            Ok(item) => item,
            Err(_) if watch.interruption().is_some() => break,
            Err(error) => return Err(gix_command("status", gix::Error::from_error(error))),
        };
        if !tally.add(&item, cap) {
            return Ok((tally, true));
        }
    }
    Ok((tally, false))
}

fn status_iter(
    repo: &gix::Repository,
    stop: std::sync::Arc<AtomicBool>,
) -> Result<gix::status::Iter, GitError> {
    repo.status(gix::progress::Discard)
        .map_err(|error| gix_command("status", error))?
        .should_interrupt_owned(stop)
        .untracked_files(UntrackedFiles::Collapsed)
        .index_worktree_rewrites(None)
        .index_worktree_submodules(Submodule::Given {
            ignore: Ignore::Dirty,
            check_dirty: false,
        })
        .tree_index_track_renames(TrackRenames::Disabled)
        .into_iter(Vec::<BString>::new())
        .map_err(|error| gix_command("status", error))
}

fn has_unmerged(repo: &gix::Repository) -> Result<bool, GitError> {
    let index = repo
        .index_or_empty()
        .map_err(|error| gix_command("status", error))?;
    Ok(index
        .entries()
        .iter()
        .any(|entry| entry.flags.stage() != gix::index::entry::Stage::Unconflicted))
}
