use std::path::Path;
use std::sync::atomic::AtomicBool;

use gix::bstr::BString;
use gix::status::{Submodule, UntrackedFiles, tree_index::TrackRenames};
use gix::submodule::config::Ignore;

use super::{GixGit, gix_command};
use crate::GitError;
use crate::cli::{Cancel, StatusLimits};
use crate::status::{GitStatus, RepoStatus, fetched_ms, operation};
use crate::trust::Guard;

mod filters;
mod lfs;
mod refs;
mod tally;
mod watch;

use filters::Filters;
use lfs::{Pointers, Verdict};
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
        if let Some(error) = self.guard_refusal(guard) {
            return Err(error);
        }
        if cancel.is_cancelled() {
            return Err(GitError::Cancelled);
        }
        let mut repo = self.open()?;
        let lfs = match filters::classify(&repo) {
            Filters::Clear => false,
            Filters::Lfs => true,
            Filters::Foreign(filter) => return Err(self.filter_refused(filter)),
        };
        if lfs {
            lfs::drop_driver(&mut repo)?;
        }
        let watch = Watch::start(cancel, limits.timeout).map_err(|error| GitError::Io {
            path: self.repo.clone(),
            kind: error.kind(),
        })?;
        let result = read_status(
            &repo,
            &watch,
            limits.stdout_cap,
            lfs.then_some(self.repo.as_path()),
        );
        match watch.interruption() {
            Some(error) => Err(error),
            None => result,
        }
    }

    /// What the repository's own config forbids: a blocked guard, a filter driver or a program.
    fn guard_refusal(&self, guard: &Guard) -> Option<GitError> {
        let path = self.repo.clone();
        if guard.is_blocked() {
            return Some(GitError::NotTrusted { path });
        }
        let keys = guard.keys();
        if let Some(key) = keys.iter().find(|key| key.starts_with("filter.")) {
            return Some(self.filter_refused(filter_of(key)));
        }
        keys.iter()
            .any(|key| REFUSED_KEYS.contains(key))
            .then_some(GitError::ProgramConfigured { path })
    }

    fn filter_refused(&self, filter: String) -> GitError {
        GitError::FilterRefused {
            path: self.repo.clone(),
            filter,
        }
    }
}

/// The driver name in a guard key such as `filter.crypt.clean`.
fn filter_of(key: &str) -> String {
    let rest = key.strip_prefix("filter.").unwrap_or(key);
    rest.rsplit_once('.')
        .map_or(rest, |(name, _)| name)
        .to_owned()
}

/// `lfs_root` is set when the standard git-lfs driver was dropped from `repo`'s config.
fn read_status(
    repo: &gix::Repository,
    watch: &Watch,
    cap: usize,
    lfs_root: Option<&Path>,
) -> Result<RepoStatus, GitError> {
    let head = refs::head(repo)?;
    let upstream = refs::upstream(repo, &head, watch.stopped())?;
    let (tally, capped) = collect(repo, watch, cap, lfs_root)?;
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

fn collect(
    repo: &gix::Repository,
    watch: &Watch,
    cap: usize,
    lfs_root: Option<&Path>,
) -> Result<(Tally, bool), GitError> {
    let items = status_iter(repo, watch.flag())?;
    let mut pointers = lfs_root.map(|root| Pointers::new(repo, root)).transpose()?;
    let mut tally = Tally::default();
    for item in items {
        let item = match item {
            Ok(item) => item,
            Err(_) if watch.interruption().is_some() => break,
            Err(error) => return Err(gix_command("status", gix::Error::from_error(error))),
        };
        if let Some(pointers) = pointers.as_mut() {
            match pointers.verdict(&item, watch.stopped())? {
                Verdict::Unchanged => continue,
                Verdict::Stopped => break,
                Verdict::Modified => {}
            }
        }
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
