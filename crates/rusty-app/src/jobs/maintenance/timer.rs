use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use rusty_core::retention::RetentionPolicy;
use rusty_fs::{CancelToken, ProviderOps};
use rusty_retention::{MaintenanceRequest, RetentionRun, Vault};
use rusty_store::{RetentionCatalog, StoreDirs};

use super::super::mutation_lock::{LeaseError, Mode, Wait, lease};

const FIRST_PASS: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY: Duration = Duration::from_secs(10 * 60);
const STOP_WAIT: Duration = Duration::from_secs(1);
const MAX_PURGES: u16 = 256;
const PAGE: u32 = 1000;

mod inventory;
use inventory::Live;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Pressure,
    Scheduled,
}

pub(crate) type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Schedule {
    pub every: Duration,
    pub retry: Duration,
    pub now: Clock,
    pub memory: Option<crate::files::SharedDb>,
    pub retention: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            every: EVERY,
            retry: RETRY,
            now: Arc::new(super::super::now_ms),
            memory: None,
            retention: true,
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
    #[cfg(test)]
    pub(crate) fn start(
        dirs: Option<&StoreDirs>,
        retention: bool,
        ops: Arc<dyn ProviderOps>,
    ) -> Self {
        Self::start_with_memory((dirs, None), retention, ops)
    }

    pub(crate) fn start_with_memory(
        (dirs, memory): (Option<&StoreDirs>, Option<crate::files::SharedDb>),
        retention: bool,
        ops: Arc<dyn ProviderOps>,
    ) -> Self {
        let dirs = dirs.filter(|_| retention || memory.is_some());
        Self::with(
            dirs,
            ops,
            Schedule {
                memory,
                retention,
                ..Schedule::default()
            },
        )
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
                run(&dirs, ops.as_ref(), schedule, &token);
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

fn run(dirs: &StoreDirs, ops: &dyn ProviderOps, mut schedule: Schedule, token: &CancelToken) {
    run_with_pause((dirs, ops), (&mut schedule, token), |delay| {
        !token.wait_cancelled(delay)
    });
}

fn run_with_pause(
    (dirs, ops): (&StoreDirs, &dyn ProviderOps),
    (schedule, token): (&mut Schedule, &CancelToken),
    pause: impl FnMut(Duration) -> bool,
) {
    if schedule.retention {
        pass_with(
            (dirs, ops),
            ((schedule.now)(), token),
            (Scope::Pressure, schedule.memory.as_ref()),
        );
    }
    cycle(&schedule.clone(), pause, |now| {
        scheduled_pass((dirs, ops), schedule, (now, token))
    });
}

fn scheduled_pass(
    (dirs, ops): (&StoreDirs, &dyn ProviderOps),
    schedule: &mut Schedule,
    (now, token): (i64, &CancelToken),
) -> Pass {
    if let Some(memory) = schedule.memory.take() {
        crate::files::prune_memory(&memory);
    }
    if schedule.retention {
        return pass(dirs, ops, (now, token));
    }
    token.cancel();
    Pass::Ran {
        volumes: 0,
        purged: 0,
        bytes: 0,
        pruned: 0,
    }
}

pub(crate) fn cycle(
    schedule: &Schedule,
    mut pause: impl FnMut(Duration) -> bool,
    mut pass: impl FnMut(i64) -> Pass,
) {
    let mut delay = FIRST_PASS;
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
    pass_with((dirs, ops), (now, cancel), (Scope::Scheduled, None))
}

fn pass_with(
    (dirs, ops): (&StoreDirs, &dyn ProviderOps),
    (now, cancel): (i64, &CancelToken),
    (scope, journal): (Scope, Option<&crate::jobs::SharedJournal>),
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
    match maintain((dirs, ops), (policy, now, cancel), (scope, journal)) {
        Ok(done) => done,
        Err(error) => {
            tracing::warn!(%error, "retention maintenance pass failed");
            Pass::Failed
        }
    }
}

fn maintain(
    (dirs, ops): (&StoreDirs, &dyn ProviderOps),
    (policy, now, cancel): (RetentionPolicy, i64, &CancelToken),
    (scope, journal): (Scope, Option<&crate::jobs::SharedJournal>),
) -> Result<Pass, String> {
    let mut live = live(dirs, ops, journal)?;
    if scope == Scope::Pressure {
        live.retain_pressured(ops, policy)?;
        if live.volumes.is_empty() {
            return Ok(Pass::Ran {
                volumes: 0,
                purged: 0,
                bytes: 0,
                pruned: 0,
            });
        }
    }
    let (db, _) = crate::crash::open_db(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let mut vault = Vault::new(db, &dirs.data, ops).map_err(|error| error.to_string())?;
    let selection_time = if scope == Scope::Pressure { 0 } else { now };
    let (purged, bytes) = purge_volumes(&mut vault, &live, (policy, selection_time, cancel));
    drop(vault);
    let pruned = if scope == Scope::Scheduled {
        prune(dirs, now)
    } else {
        0
    };
    tracing::info!(
        volumes = live.volumes.len(),
        purged,
        bytes,
        pruned,
        "retention maintenance pass finished"
    );
    Ok(Pass::Ran {
        volumes: live.volumes.len(),
        purged,
        bytes,
        pruned,
    })
}

fn purge_volumes(
    vault: &mut Vault<'_>,
    live: &Live,
    (policy, now, cancel): (RetentionPolicy, i64, &CancelToken),
) -> (usize, u64) {
    let (mut purged, mut bytes) = (0, 0_u64);
    for at in live.volumes.values() {
        let request = &MaintenanceRequest {
            at,
            policy,
            now,
            max_purges: MAX_PURGES,
        };
        match vault.maintain_volume(request, &mut RetentionRun::new(cancel, &mut |_| {})) {
            Ok(report) => {
                purged += report.purged.len();
                bytes = report.purged.iter().fold(bytes, |total, id| {
                    total.saturating_add(live.sizes.get(id).copied().unwrap_or(0))
                });
            }
            Err(error) => tracing::warn!(%error, "retention volume maintenance deferred"),
        }
    }
    (purged, bytes)
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

fn live(
    dirs: &StoreDirs,
    ops: &dyn ProviderOps,
    journal: Option<&crate::jobs::SharedJournal>,
) -> Result<Live, String> {
    let catalog = inventory::Catalog::open(dirs, journal)?;
    let mut live = Live::default();
    let mut after = 0;
    loop {
        let page = catalog.page(after)?;
        let Some(last) = page.last() else {
            return Ok(live);
        };
        after = last.id.0;
        for entry in page {
            let bytes = entry.record.bytes().unwrap_or(0);
            live.sizes.insert(entry.id, bytes);
            let expected = entry.record.parent_identity;
            *live.totals.entry(expected.volume).or_default() += u128::from(bytes);
            if live.volumes.contains_key(&expected.volume) {
                continue;
            }
            let Some(parent) = entry.record.anchor() else {
                continue;
            };
            if ops.file_identity(&parent).ok().flatten() == Some(expected) {
                live.volumes.insert(expected.volume, parent);
            }
        }
    }
}

#[cfg(test)]
mod tests;
