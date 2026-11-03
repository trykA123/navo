use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError, Weak, mpsc};
use std::time::Duration;

use navo_core::Location;
use navo_fs::ProviderOps;
use navo_store::{JobJournal, JournalError, PruneLimits, VacuumOnce};

use super::{SharedJournal, lock, now_ms};

#[cfg(not(test))]
const DEBOUNCE: Duration = Duration::from_secs(5);
#[cfg(test)]
const DEBOUNCE: Duration = Duration::from_millis(20);
const QUEUE: usize = 64;
const RECLAIM_PAGES: u32 = 1024;
// One trim returns at most 4 GiB of 4 KiB pages; the rest waits for the next trim.
const RECLAIM_ROUNDS: u32 = 1024;

static RECOVERING: Mutex<Vec<usize>> = Mutex::new(Vec::new());
type Request = Weak<Mutex<JobJournal>>;

// Test journals trim only when a test asks; a Weak keeps the address from being reused.
#[cfg(test)]
static ARMED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trim {
    Done { pruned: u32 },
    Deferred,
}

/// Blocks runtime trims of `journal` while crash recovery reads and settles its jobs.
pub(crate) struct Recovering(usize);

pub(crate) fn recovering(journal: &SharedJournal) -> Recovering {
    let key = key(journal);
    registry(&RECOVERING).push(key);
    Recovering(key)
}

impl Drop for Recovering {
    fn drop(&mut self) {
        let mut keys = registry(&RECOVERING);
        if let Some(at) = keys.iter().position(|key| *key == self.0) {
            keys.swap_remove(at);
        }
    }
}

/// Startup trim after recovery: prune, the one VACUUM that enables incremental vacuum, reclaim.
pub(crate) fn at_startup(
    journal: &SharedJournal,
    data: &Path,
    ops: &dyn ProviderOps,
) -> Result<(), JournalError> {
    let pruned = lock(journal).prune(PruneLimits::standard(now_ms()))?;
    let free = data
        .to_str()
        .and_then(|text| Location::new(text).ok())
        .and_then(|at| ops.available_space(&at).ok().flatten());
    match free.map(|free| lock(journal).vacuum_once(free)) {
        None => tracing::info!("journal vacuum waits: free space is unknown"),
        Some(Ok(VacuumOnce::AlreadyIncremental)) => {}
        Some(Ok(outcome)) => tracing::info!(?outcome, "journal vacuum checked"),
        Some(Err(error)) => tracing::warn!(%error, "journal vacuum failed; retried next start"),
    }
    let pages = reclaim(journal, Wait::Patient);
    tracing::info!(pruned, pages, "journal trimmed at startup");
    Ok(())
}

/// Asks for a trim a few seconds after the last finished job, on a background thread.
pub(crate) fn after_job(journal: &SharedJournal) {
    #[cfg(test)]
    if !ARMED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|armed| armed.as_ptr() == Arc::as_ptr(journal))
    {
        return;
    }
    if let Some(sender) = worker() {
        let _ = sender.try_send(Arc::downgrade(journal));
    }
}

#[cfg(test)]
pub(crate) fn arm(journal: &SharedJournal) {
    ARMED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Arc::downgrade(journal));
}

/// Prunes and reclaims unless recovery runs, another caller holds the journal, or another
/// connection holds the database write lock; those trims are deferred and never wait.
pub(crate) fn trim(journal: &SharedJournal) -> Trim {
    if registry(&RECOVERING).contains(&key(journal)) {
        return Trim::Deferred;
    }
    let Some(held) = try_lock(journal) else {
        return Trim::Deferred;
    };
    let pruned = held.prune_if_idle(PruneLimits::standard(now_ms()));
    drop(held);
    let pruned = match pruned {
        Ok(Some(pruned)) => pruned,
        Ok(None) => return Trim::Deferred,
        Err(error) => {
            tracing::warn!(%error, "journal prune failed");
            0
        }
    };
    let pages = reclaim(journal, Wait::Never);
    tracing::info!(pruned, pages, "journal trimmed");
    Trim::Done { pruned }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wait {
    Patient,
    Never,
}

fn reclaim(journal: &SharedJournal, wait: Wait) -> u64 {
    let mut left = 0;
    for _ in 0..RECLAIM_ROUNDS {
        let Some(held) = try_lock(journal) else {
            return left;
        };
        let step = match wait {
            Wait::Patient => held.reclaim(RECLAIM_PAGES).map(Some),
            Wait::Never => held.reclaim_if_idle(RECLAIM_PAGES),
        };
        match step {
            Ok(None) => return left,
            Ok(Some(0)) => {
                left = 0;
                break;
            }
            Ok(Some(pages)) => left = pages,
            Err(error) => {
                tracing::warn!(%error, "journal pages not reclaimed");
                return left;
            }
        }
    }
    if let Some(held) = try_lock(journal) {
        match held.try_truncate_wal() {
            Ok(true) => {}
            Ok(false) => tracing::debug!("journal WAL busy; truncated later"),
            Err(error) => tracing::warn!(%error, "journal WAL not truncated"),
        }
    }
    left
}

fn worker() -> Option<&'static mpsc::SyncSender<Request>> {
    static SENDER: OnceLock<Option<mpsc::SyncSender<Request>>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel(QUEUE);
            std::thread::Builder::new()
                .name("journal-trim".into())
                .spawn(move || work(&receiver))
                .inspect_err(|error| tracing::warn!(%error, "journal trim worker did not start"))
                .ok()
                .map(|_| sender)
        })
        .as_ref()
}

fn work(receiver: &mpsc::Receiver<Request>) {
    let mut waiting: Vec<Request> = Vec::new();
    loop {
        let next = if waiting.is_empty() {
            receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            receiver.recv_timeout(DEBOUNCE)
        };
        match next {
            Ok(journal) => {
                if !waiting.iter().any(|old| old.ptr_eq(&journal)) {
                    waiting.push(journal);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => waiting.retain(|weak| {
                weak.upgrade()
                    .is_some_and(|journal| trim(&journal) == Trim::Deferred)
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn try_lock(journal: &SharedJournal) -> Option<MutexGuard<'_, JobJournal>> {
    match journal.try_lock() {
        Ok(held) => Some(held),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn key(journal: &SharedJournal) -> usize {
    Arc::as_ptr(journal).addr()
}

fn registry(keys: &Mutex<Vec<usize>>) -> MutexGuard<'_, Vec<usize>> {
    keys.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod scale_tests;
#[cfg(test)]
mod tests;
