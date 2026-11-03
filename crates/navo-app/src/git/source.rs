use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use navo_git::cli::{Cancel as GitCancel, CliGit};
use navo_git::status::{GitStatus, RepoStatus};
use navo_git::trust::{Refusal, refusal};
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

/// Repositories remembered on the Git command line before the memory is cleared.
const CLI_MEMORY: usize = 256;

pub(crate) struct GitSource<C, G> {
    cli: C,
    gix: G,
    engine: GitEngine,
    on_cli: Mutex<HashSet<PathBuf>>,
}

impl<C: Backend, G: Backend> GitSource<C, G> {
    pub(crate) fn new(cli: C, gix: G, engine: GitEngine) -> Self {
        Self {
            cli,
            gix,
            engine,
            on_cli: Mutex::default(),
        }
    }

    fn read(&self, repo: &Path, git: &GitCancel) -> Result<RepoStatus, GitError> {
        match self.engine {
            GitEngine::Git => self.cli.open(repo)?.status(git),
            GitEngine::Gix => self.gix.open(repo)?.status(git),
            GitEngine::Auto => self.read_auto(repo, git),
        }
    }

    /// Gix first. A repository gix cannot read safely moves to the CLI, and stays there for this
    /// scan. Without a usable Git the refusal reads as `NoGit`.
    fn read_auto(&self, repo: &Path, git: &GitCancel) -> Result<RepoStatus, GitError> {
        if !self.is_on_cli(repo) {
            let refused = match self.gix.open(repo).and_then(|reader| reader.status(git)) {
                Err(error) if cli_can_read(&error) => error,
                other => return other,
            };
            tracing::debug!(repo = %repo.display(), %refused, "gix refused, using the Git command line");
            self.remember_cli(repo);
        }
        match self.cli.open(repo) {
            Err(GitError::GitNotFound | GitError::GitTooOld { .. }) => Err(GitError::GitNotFound),
            other => other?.status(git),
        }
    }

    fn is_on_cli(&self, repo: &Path) -> bool {
        self.on_cli
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(repo)
    }

    fn remember_cli(&self, repo: &Path) {
        let mut chosen = self.on_cli.lock().unwrap_or_else(PoisonError::into_inner);
        if chosen.len() >= CLI_MEMORY {
            chosen.clear();
        }
        chosen.insert(repo.to_path_buf());
    }
}

/// Refusals the Git command line handles: it neutralises filters and fsmonitor, and applies the
/// user's `safe.directory`. A blocked guard stays refused, since the CLI blocks on it too.
fn cli_can_read(error: &GitError) -> bool {
    matches!(
        refusal(error),
        Some(Refusal::Ownership | Refusal::Filter | Refusal::Program)
    ) || matches!(error, GitError::Unsupported { .. })
}

impl<C: Backend, G: Backend> StatusSource for GitSource<C, G> {
    fn status(&self, repo_root: &Path, cancel: &Cancel) -> Result<RepoStatus, ScanError> {
        bridged(cancel, |git| self.read(repo_root, git)).map_err(|error| scan_error(&error))
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
        other => refusal(other).map_or(ScanError::Failed, ScanError::Refused),
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
