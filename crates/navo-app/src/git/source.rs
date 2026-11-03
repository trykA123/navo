use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use navo_git::cli::{Cancel as GitCancel, CliGit};
use navo_git::status::{GitStatus, RepoStatus};
use navo_git::trust::untrusted_reason;
use navo_git::{GitError, GixGit};
use navo_store::GitEngine;

use super::scan::{Cancel, ScanError, StatusSource};

const CANCEL_POLL: Duration = Duration::from_millis(10);

/// One opened repository: the trust guard is read before any status call.
pub(crate) trait Reader {
    fn status(&self, cancel: &GitCancel) -> Result<RepoStatus, GitError>;
}

pub(crate) trait Backend: Send + Sync + 'static {
    /// Fails with `GitNotFound` or `GitTooOld` when no usable Git exists.
    fn open(&self, repo: &Path) -> Result<Box<dyn Reader>, GitError>;
}

#[derive(Debug)]
pub(crate) struct CliBackend;

struct CliReader(CliGit);

impl Backend for CliBackend {
    fn open(&self, repo: &Path) -> Result<Box<dyn Reader>, GitError> {
        Ok(Box::new(CliReader(CliGit::new(repo)?)))
    }
}

impl Reader for CliReader {
    fn status(&self, cancel: &GitCancel) -> Result<RepoStatus, GitError> {
        let guard = self.0.guard()?;
        self.0.status(&guard, cancel)
    }
}

pub(crate) struct GitSource<C, G> {
    cli: C,
    gix: G,
    engine: GitEngine,
}

impl<C: Backend, G: Backend> GitSource<C, G> {
    pub(crate) fn new(cli: C, gix: G, engine: GitEngine) -> Self {
        Self { cli, gix, engine }
    }

    /// `Git` never leaves the CLI; `Auto` falls back to gix only when no usable Git exists.
    fn open(&self, repo: &Path) -> Result<Box<dyn Reader>, GitError> {
        match self.engine {
            GitEngine::Git => self.cli.open(repo),
            GitEngine::Gix => self.gix.open(repo),
            GitEngine::Auto => match self.cli.open(repo) {
                Err(GitError::GitNotFound | GitError::GitTooOld { .. }) => self.gix.open(repo),
                other => other,
            },
        }
    }
}

impl<C: Backend, G: Backend> StatusSource for GitSource<C, G> {
    fn status(&self, repo_root: &Path, cancel: &Cancel) -> Result<RepoStatus, ScanError> {
        let reader = self.open(repo_root).map_err(|error| scan_error(&error))?;
        bridged(cancel, |git| reader.status(git)).map_err(|error| scan_error(&error))
    }
}

#[derive(Debug)]
pub(crate) struct GixBackend;

struct GixReader(GixGit);

impl Backend for GixBackend {
    fn open(&self, repo: &Path) -> Result<Box<dyn Reader>, GitError> {
        Ok(Box::new(GixReader(GixGit::new(repo)?)))
    }
}

impl Reader for GixReader {
    fn status(&self, cancel: &GitCancel) -> Result<RepoStatus, GitError> {
        let guard = self.0.guard()?;
        self.0.status(&guard, cancel)
    }
}

fn scan_error(error: &GitError) -> ScanError {
    match error {
        GitError::GitNotFound | GitError::GitTooOld { .. } => ScanError::NoGit,
        GitError::Timeout => ScanError::TimedOut,
        GitError::Cancelled => ScanError::Cancelled,
        other if untrusted_reason(other).is_some() => ScanError::NotTrusted,
        _ => ScanError::Failed,
    }
}

/// Runs `run` with a Git cancel that follows `cancel` within `CANCEL_POLL`.
pub(super) fn bridged<T>(cancel: &Cancel, run: impl FnOnce(&GitCancel) -> T) -> T {
    let git = GitCancel::default();
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        let watcher = thread::Builder::new()
            .name("git-cancel".to_owned())
            .spawn_scoped(scope, || follow(cancel, &git, &done));
        if let Err(error) = &watcher {
            tracing::warn!(%error, "git cancel watcher did not start");
        }
        let out = run(&git);
        done.store(true, Ordering::Release);
        out
    })
}

fn follow(cancel: &Cancel, git: &GitCancel, done: &AtomicBool) {
    while !done.load(Ordering::Acquire) {
        if cancel.is_cancelled() {
            git.cancel();
            return;
        }
        thread::sleep(CANCEL_POLL);
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
