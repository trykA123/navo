#[path = "scan_cache.rs"]
mod cache;
#[path = "scan_listing.rs"]
mod listing;
#[path = "scan_plan.rs"]
mod plan;
#[path = "scan_queue.rs"]
mod queue;
#[path = "scan_saved.rs"]
mod saved;
#[path = "scan_skip.rs"]
mod skip;
#[path = "scan_slots.rs"]
mod slots;
#[path = "scan_system.rs"]
mod system;
#[path = "scan_view.rs"]
mod view;
#[path = "scan_watch.rs"]
mod watches;
#[path = "scan_worker.rs"]
mod worker;

use std::io;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_core::{FileIdentity, Location};
use navo_fs::DriveClass;
use navo_git::status::{Fingerprint, RepoStatus};
use navo_git::trust::Refusal;
use navo_store::GitSettings;
use navo_ui::git::FolderBadges;

pub(crate) use navo_fs::CancelToken as Cancel;
use queue::{Listing, Shared};
pub(crate) use saved::{DbSaved, SavedBadges};
pub(crate) use slots::Slots;
pub(crate) use system::SystemProbe;
pub(crate) use view::{ScanRepo, ScanView};

const QUEUE_CAP: usize = 256;
const CACHE_CAP: usize = 256;
const ROW_CAP: usize = 2000;
const REPO_CAP: usize = 64;
/// Most watches or kept proofs held at once.
const WATCH_CAP: usize = 64;
const MIN_PROCESSES: usize = 2;
const MAX_PROCESSES: usize = 4;
const CACHE_BYTES: usize = 1024 * 1024;
const QUEUE_BYTES: usize = 1024 * 1024;
const STOP_WAIT: Duration = Duration::from_secs(2);
const TICK: Duration = Duration::from_millis(10);

type Notify = Box<dyn Fn(u64) + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ScanError {
    #[error("repository status timed out")]
    TimedOut,
    #[error("Git is unavailable")]
    NoGit,
    #[error("repository refused: {0:?}")]
    Refused(Refusal),
    #[error("repository status failed")]
    Failed,
    #[error("repository status was cancelled")]
    Cancelled,
}

/// Sources must observe cancellation within 100 ms and bound their output to 8 MiB.
pub(crate) trait StatusSource: Send + Sync + 'static {
    fn status(&self, repo_root: &Path, cancel: &Cancel) -> Result<RepoStatus, ScanError>;
}

/// One repository tree watch: live while the backend covers the subtree, counting every raw
/// notification before the debouncer.
pub(crate) trait RepoWatch: Send {
    fn is_live(&self) -> bool;
    fn changes(&self) -> u64;
}

impl RepoWatch for navo_fs::Watch {
    fn is_live(&self) -> bool {
        navo_fs::Watch::is_live(self)
    }

    fn changes(&self) -> u64 {
        self.raw_changes()
    }
}

pub(crate) trait Probe: Send + Sync + 'static {
    fn has_git(&self, folder: &Path) -> bool;
    fn drive_class(&self, listing: &Location) -> DriveClass;
    fn now(&self) -> Instant {
        Instant::now()
    }

    /// The files a status read, or `None` when no proof is safe.
    fn fingerprint(&self, _root: &Path, _upstream: Option<&str>) -> Option<Fingerprint> {
        None
    }

    /// A subtree watch on `root`, or `None` when the platform cannot provide one.
    fn watch_tree(&self, _root: &Path) -> Option<Box<dyn RepoWatch>> {
        None
    }

    /// The volume class of `root`, `Unknown` when it cannot be proven a fixed local volume.
    fn root_class(&self, _root: &Path, _listing: &Location) -> DriveClass {
        DriveClass::Unknown
    }

    /// The file identity of `root`, used to notice a replaced folder.
    fn identity(&self, _root: &Path) -> Option<FileIdentity> {
        None
    }
}

pub(crate) struct Ports {
    pub source: Arc<dyn StatusSource>,
    pub probe: Arc<dyn Probe>,
    /// Keeps the last statuses across restarts of Navo and of the scanner.
    pub saved: Option<Arc<dyn SavedBadges>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub name: String,
    pub on_screen: bool,
}

#[derive(Debug, Clone, Copy)]
struct Limits {
    status_timeout: Duration,
    probe_timeout: Duration,
    /// Git processes at once.
    processes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            status_timeout: Duration::from_secs(10),
            probe_timeout: Duration::from_millis(500),
            processes: MIN_PROCESSES,
        }
    }
}

/// Git processes at once: half the cores, from two to four.
fn process_cap(cores: usize) -> usize {
    (cores / 2).clamp(MIN_PROCESSES, MAX_PROCESSES)
}

pub(crate) struct GitScan {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    running: Mutex<Option<worker::Running>>,
    slots: Slots,
}

impl GitScan {
    pub(crate) fn start(ports: Ports, settings: &GitSettings, notify: Notify) -> io::Result<Self> {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        let limits = Limits {
            processes: process_cap(cores),
            ..Limits::default()
        };
        Self::start_with_limits(ports, settings, notify, limits)
    }

    fn start_with_limits(
        ports: Ports,
        settings: &GitSettings,
        notify: Notify,
        limits: Limits,
    ) -> io::Result<Self> {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let slots = Slots::new(limits.processes);
        let running = if settings.badges {
            Some(worker::spawn(
                shared.clone(),
                ports,
                worker::Config {
                    network: settings.network,
                    limits,
                    notify,
                    slots: slots.clone(),
                },
            )?)
        } else {
            None
        };
        Ok(Self {
            shared,
            running: Mutex::new(running),
            slots,
        })
    }

    /// Numbers later publishes above `revision`; call before the first `observe`.
    pub(crate) fn resume_after(&self, revision: u64) {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.resume_after(revision);
    }

    pub(crate) fn view(&self) -> ScanView {
        ScanView {
            shared: self.shared.clone(),
        }
    }

    pub(crate) fn slots(&self) -> Slots {
        self.slots.clone()
    }

    /// Rows marked on screen precede other rows at both probe and repository caps.
    pub(crate) fn observe(&self, folder: &Location, generation: u64, rows: Vec<Row>) {
        if !folder.is_local()
            || self
                .running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_none()
        {
            return;
        }
        let Some(listing) = Listing::new(folder, generation, rows) else {
            return;
        };
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.observe(listing);
        self.shared.1.notify_one();
    }

    pub(crate) fn forget(&self, folder: &Location) {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.forget(folder);
        self.shared.1.notify_one();
    }

    pub(crate) fn rescan(&self, folder: &Location) {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.rescan(folder);
        self.shared.1.notify_one();
    }

    /// Rechecks only the roots whose watch counted a change.
    pub(crate) fn rescan_changed(&self, folder: &Location) {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.rescan_changed(folder);
        self.shared.1.notify_one();
    }

    pub(crate) fn badges(&self, folder: &Location) -> Option<Arc<FolderBadges>> {
        self.shared
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .active
            .get(folder)
            .and_then(|entry| entry.badges.clone())
    }

    pub(crate) fn stop(&self) -> bool {
        let running = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(running) = running else {
            return true;
        };
        {
            let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
            shared.stop();
            self.shared.1.notify_one();
        }
        running.stop()
    }
}

impl Drop for GitScan {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
