use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use rusty_core::Location;
use rusty_core::retention::{RetentionId, RetentionPolicy};
use rusty_fs::{CancelToken, ProviderOps};
use rusty_retention::{MaintenanceRequest, RetentionRun, Vault};
use rusty_store::{RetentionCatalog, StoreDirs};

use super::super::mutation_lock::{LeaseError, Mode, Wait, lease};

const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY: Duration = Duration::from_secs(10 * 60);
const STOP_WAIT: Duration = Duration::from_secs(1);
const MAX_PURGES: u16 = 256;
const PAGE: u32 = 1000;

pub(crate) type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Schedule {
    pub every: Duration,
    pub retry: Duration,
    pub now: Clock,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            every: EVERY,
            retry: RETRY,
            now: Arc::new(super::super::now_ms),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pass {
    Ran {
        volumes: usize,
        purged: usize,
        bytes: u64,
        pruned: usize,
    },
    Busy,
    Failed,
}

struct Worker {
    cancel: CancelToken,
    done: mpsc::Receiver<()>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct MaintenanceTimer {
    worker: Mutex<Option<Worker>>,
}

impl MaintenanceTimer {
    pub(crate) fn start(
        dirs: Option<&StoreDirs>,
        retention: bool,
        ops: Arc<dyn ProviderOps>,
    ) -> Self {
        Self::with(dirs.filter(|_| retention), ops, Schedule::default())
    }

    pub(crate) fn with(
        dirs: Option<&StoreDirs>,
        ops: Arc<dyn ProviderOps>,
        schedule: Schedule,
    ) -> Self {
        let Some(dirs) = dirs.cloned() else {
            return Self::default();
        };
        let cancel = CancelToken::new();
        let (sender, done) = mpsc::channel();
        let token = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("retention-maintenance".into())
            .spawn(move || {
                cycle(
                    &schedule,
                    |delay| !token.wait_cancelled(delay),
                    |now| pass(&dirs, ops.as_ref(), (now, &token)),
                );
                let _ = sender.send(());
            });
        match spawned {
            Ok(thread) => Self {
                worker: Mutex::new(Some(Worker {
                    cancel,
                    done,
                    thread,
                })),
            },
            Err(error) => {
                tracing::warn!(%error, "retention maintenance timer did not start");
                Self::default()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_running(&self) -> bool {
        self.worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    pub(crate) fn stop(&self) {
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(worker) = worker else {
            return;
        };
        worker.cancel.cancel();
        if worker.done.recv_timeout(STOP_WAIT) == Err(mpsc::RecvTimeoutError::Timeout) {
            tracing::warn!("retention maintenance still running at exit");
            return;
        }
        if worker.thread.join().is_err() {
            tracing::warn!("retention maintenance thread failed");
        }
    }
}

pub(crate) fn cycle(
    schedule: &Schedule,
    mut pause: impl FnMut(Duration) -> bool,
    mut pass: impl FnMut(i64) -> Pass,
) {
    let mut delay = Duration::ZERO;
    while pause(delay) {
        delay = match pass((schedule.now)()) {
            Pass::Busy | Pass::Failed => schedule.retry,
            Pass::Ran { .. } => schedule.every,
        };
    }
}

pub(crate) fn pass(
    dirs: &StoreDirs,
    ops: &dyn ProviderOps,
    (now, cancel): (i64, &CancelToken),
) -> Pass {
    let policy = match super::super::profile::settings(dirs) {
        Ok(settings) => settings.policy(),
        Err(error) => {
            tracing::warn!(%error, "retention maintenance cannot read settings");
            return Pass::Failed;
        }
    };
    let wait = Wait {
        cancel: Some(cancel),
        waiting: None,
        limit: Duration::ZERO,
    };
    let _lease = match lease(&dirs.data, Mode::Exclusive, wait) {
        Ok(file) => file,
        Err(LeaseError::Busy) => {
            tracing::info!("retention maintenance deferred: a job holds the lease");
            return Pass::Busy;
        }
        Err(error) => {
            tracing::warn!(error = %String::from(error), "retention maintenance skipped");
            return Pass::Failed;
        }
    };
    match maintain(dirs, ops, (policy, now, cancel)) {
        Ok(done) => done,
        Err(error) => {
            tracing::warn!(%error, "retention maintenance pass failed");
            Pass::Failed
        }
    }
}

fn maintain(
    dirs: &StoreDirs,
    ops: &dyn ProviderOps,
    (policy, now, cancel): (RetentionPolicy, i64, &CancelToken),
) -> Result<Pass, String> {
    let (volumes, sizes) = live(dirs, ops)?;
    let (db, _) = crate::crash::open_db(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let mut vault = Vault::new(db, &dirs.data, ops).map_err(|error| error.to_string())?;
    let (mut purged, mut bytes) = (0, 0_u64);
    for at in volumes.values() {
        let request = MaintenanceRequest {
            at,
            policy,
            now,
            max_purges: MAX_PURGES,
        };
        match vault.maintain_volume(request, &mut RetentionRun::new(cancel, &mut |_| {})) {
            Ok(report) => {
                purged += report.purged.len();
                bytes = report.purged.iter().fold(bytes, |total, id| {
                    total.saturating_add(sizes.get(id).copied().unwrap_or(0))
                });
            }
            Err(error) => tracing::warn!(%error, "retention volume maintenance deferred"),
        }
    }
    drop(vault);
    let pruned = prune(dirs, now);
    tracing::info!(
        volumes = volumes.len(),
        purged,
        bytes,
        pruned,
        "retention maintenance pass finished"
    );
    Ok(Pass::Ran {
        volumes: volumes.len(),
        purged,
        bytes,
        pruned,
    })
}

fn prune(dirs: &StoreDirs, now: i64) -> usize {
    let pruned = crate::crash::open_db(&dirs.journal_file())
        .map_err(|error| error.to_string())
        .and_then(|(db, _)| RetentionCatalog::new(db).map_err(|error| error.to_string()))
        .and_then(|mut catalog| {
            catalog
                .prune_terminal(now)
                .map_err(|error| error.to_string())
        });
    pruned.unwrap_or_else(|error| {
        tracing::warn!(%error, "retention terminal rows not pruned");
        0
    })
}

type Live = (HashMap<u64, Location>, HashMap<RetentionId, u64>);

fn live(dirs: &StoreDirs, ops: &dyn ProviderOps) -> Result<Live, String> {
    let (db, _) = crate::crash::open_db(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let catalog = RetentionCatalog::new(db).map_err(|error| error.to_string())?;
    let (mut volumes, mut sizes) = (HashMap::new(), HashMap::new());
    let mut after = 0;
    loop {
        let page = catalog
            .live_after(after, PAGE)
            .map_err(|error| error.to_string())?;
        let Some(last) = page.last() else {
            return Ok((volumes, sizes));
        };
        after = last.id.0;
        for entry in page {
            sizes.insert(entry.id, entry.record.bytes().unwrap_or(0));
            let expected = entry.record.parent_identity;
            if volumes.contains_key(&expected.volume) {
                continue;
            }
            let Some(parent) = entry.record.source.parent() else {
                continue;
            };
            if ops.file_identity(&parent).ok().flatten() == Some(expected) {
                volumes.insert(expected.volume, parent);
            }
        }
    }
}

#[cfg(test)]
mod tests;
