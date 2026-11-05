use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use navo_core::Location;
use navo_fs::DriveClass;
use navo_git::GitError;
use navo_git::cli::CliGit;
use navo_git::trust::untrusted_reason;

use crate::git::scan::{Cancel, ScanRepo, ScanView, Slots};
use crate::git::source::bridged;

/// How one fetch ended. Only `Fetched` earns a rescan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Fetched,
    Refused,
    Failed,
    Cancelled,
}

/// Implementations must return within 100 ms of `cancel` and never prompt.
pub(crate) trait Fetcher: Send + Sync + 'static {
    fn fetch(&self, repo: &Path, cancel: &Cancel) -> Outcome;
}

pub(crate) trait Watch: Send + Sync + 'static {
    fn repos(&self) -> Vec<ScanRepo>;
    fn rescan(&self, repo: &ScanRepo);
    fn drive_class(&self, listing: &Location) -> DriveClass;
}

pub(crate) trait Clock: Send + Sync + 'static {
    /// Milliseconds since the Unix epoch, as `RepoBadge::fetched_ms` counts them.
    fn now_ms(&self) -> i64;
}

pub(crate) struct Ports {
    pub watch: Arc<dyn Watch>,
    pub fetcher: Arc<dyn Fetcher>,
    pub clock: Arc<dyn Clock>,
    pub slots: Slots,
}

impl Ports {
    pub(crate) fn system(view: ScanView, slots: Slots) -> Self {
        Self {
            watch: Arc::new(view),
            fetcher: Arc::new(CliFetcher),
            clock: Arc::new(SystemClock),
            slots,
        }
    }
}

impl Watch for ScanView {
    fn repos(&self) -> Vec<ScanRepo> {
        ScanView::repos(self)
    }

    fn rescan(&self, repo: &ScanRepo) {
        ScanView::rescan_repo(self, &repo.listing, repo.path.clone());
    }

    fn drive_class(&self, listing: &Location) -> DriveClass {
        navo_fs::drive_class(listing)
    }
}

struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
            .unwrap_or(0)
    }
}

struct CliFetcher;

impl Fetcher for CliFetcher {
    fn fetch(&self, repo: &Path, cancel: &Cancel) -> Outcome {
        let run = || {
            let git = CliGit::new(repo)?;
            let guard = git.guard()?;
            bridged(cancel, |git_cancel| git.fetch(&guard, git_cancel))
        };
        outcome(run())
    }
}

fn outcome(result: Result<(), GitError>) -> Outcome {
    match result {
        Ok(()) => Outcome::Fetched,
        Err(GitError::Cancelled) => Outcome::Cancelled,
        Err(
            GitError::Guarded { .. }
            | GitError::NoUpstream
            | GitError::DetachedHead
            | GitError::UnsafeRemote { .. }
            | GitError::GitNotFound
            | GitError::GitTooOld { .. },
        ) => Outcome::Refused,
        Err(error) if untrusted_reason(&error).is_some() => Outcome::Refused,
        Err(error) => {
            tracing::debug!(%error, "background fetch failed");
            Outcome::Failed
        }
    }
}
