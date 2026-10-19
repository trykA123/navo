pub(crate) mod codec;
mod table;
mod warm;
mod writer;

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::mpsc::Sender;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_store::{CacheDb, CacheOpen};

use crate::{CacheKey, CacheKeys, CachedListing, ListingCache};
#[cfg(test)]
pub(crate) use codec::FORMAT;
pub(crate) use table::ListingTable;
pub use warm::Warm;
#[cfg(test)]
pub(crate) use warm::admit;
use writer::Writer;

pub const DISK_BUDGET: u64 = 32 * 1024 * 1024;
const QUEUE: usize = 16;

enum Job {
    Save {
        key: CacheKey,
        listing: Arc<CachedListing>,
        is_forced: bool,
    },
    #[cfg(test)]
    Stall(Receiver<()>),
}

#[derive(Default)]
struct Progress {
    is_loaded: bool,
    accepted: u64,
    finished: u64,
    is_gone: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStats {
    pub bytes: u64,
    pub entries: u64,
    pub budget: u64,
}

#[derive(Default)]
struct Shared {
    open: Mutex<HashSet<Location>>,
    progress: Mutex<Progress>,
    moved: Condvar,
    bytes: AtomicU64,
    entries: AtomicU64,
}

impl Shared {
    fn record(&self, (bytes, entries): (u64, u64)) {
        self.bytes.store(bytes, Ordering::Relaxed);
        self.entries.store(entries, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct DiskListings {
    worker: Arc<Mutex<Worker>>,
    shared: Arc<Shared>,
}

struct Worker {
    jobs: Option<SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl DiskListings {
    #[must_use]
    pub fn open(path: &Path, warm: Warm) -> Self {
        let (jobs, inbox) = mpsc::sync_channel(QUEUE);
        let shared = Arc::<Shared>::default();
        let deadline = warm.deadline;
        let (path, writer_shared) = (path.to_path_buf(), Arc::clone(&shared));
        let spawned = std::thread::Builder::new()
            .name("listing-disk".to_owned())
            .spawn(move || run(&path, warm, inbox, &writer_shared));
        let thread = match spawned {
            Ok(thread) => {
                let left = deadline.saturating_duration_since(Instant::now());
                let progress = lock(&shared.progress);
                let _ = shared
                    .moved
                    .wait_timeout_while(progress, left, |progress| {
                        !progress.is_loaded && !progress.is_gone
                    })
                    .unwrap_or_else(PoisonError::into_inner);
                Some(thread)
            }
            Err(error) => {
                tracing::warn!(%error, "listing cache writer did not start");
                lock(&shared.progress).is_gone = true;
                None
            }
        };
        Self {
            worker: Arc::new(Mutex::new(Worker {
                jobs: Some(jobs),
                thread,
            })),
            shared,
        }
    }

    pub fn save_folders(&self, cache: &ListingCache, keys: &CacheKeys, folders: &[Location]) {
        for location in folders {
            let Some(key) = keys.key(location) else {
                continue;
            };
            if let Some(listing) = cache.lock().peek(&key) {
                self.send(Job::Save {
                    key,
                    listing,
                    is_forced: true,
                });
            }
        }
    }

    #[must_use]
    pub fn stats(&self) -> DiskStats {
        DiskStats {
            bytes: self.shared.bytes.load(Ordering::Relaxed),
            entries: self.shared.entries.load(Ordering::Relaxed),
            budget: DISK_BUDGET,
        }
    }

    #[must_use]
    pub fn flush(&self, timeout: Duration) -> bool {
        let progress = lock(&self.shared.progress);
        let target = progress.accepted;
        let (progress, _) = self
            .shared
            .moved
            .wait_timeout_while(progress, timeout, |progress| {
                !progress.is_gone && progress.finished < target
            })
            .unwrap_or_else(PoisonError::into_inner);
        !progress.is_gone && progress.finished >= target
    }

    #[must_use]
    pub fn close(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut worker = lock(&self.worker);
        worker.jobs.take();
        let Some(thread) = worker.thread.as_ref() else {
            return true;
        };
        while !thread.is_finished() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            std::thread::sleep(left.min(Duration::from_millis(5)));
        }
        worker
            .thread
            .take()
            .is_none_or(|thread| thread.join().is_ok())
    }

    pub(crate) fn save_open(&self, key: &CacheKey, listing: Arc<CachedListing>) {
        if lock(&self.shared.open).contains(&key.location) {
            self.send(Job::Save {
                key: key.clone(),
                listing,
                is_forced: false,
            });
        }
    }

    pub(crate) fn keep_open<'a>(&self, folders: impl IntoIterator<Item = &'a Location>) {
        let wanted: HashSet<&Location> = folders.into_iter().collect();
        let mut open = lock(&self.shared.open);
        if open.len() != wanted.len() || wanted.iter().any(|location| !open.contains(*location)) {
            *open = wanted.into_iter().cloned().collect();
        }
    }

    #[cfg(test)]
    pub(crate) fn stall(&self) -> Sender<()> {
        let (release, gate) = mpsc::channel();
        self.send(Job::Stall(gate));
        release
    }

    fn send(&self, job: Job) {
        let worker = lock(&self.worker);
        let mut progress = lock(&self.shared.progress);
        if worker
            .jobs
            .as_ref()
            .is_some_and(|jobs| jobs.try_send(job).is_ok())
        {
            progress.accepted += 1;
        }
    }
}

struct Gone<'a>(&'a Shared);

impl Drop for Gone<'_> {
    fn drop(&mut self) {
        lock(&self.0.progress).is_gone = true;
        self.0.moved.notify_all();
    }
}

fn run(path: &Path, warm: Warm, inbox: Receiver<Job>, shared: &Shared) {
    let _gone = Gone(shared);
    let Some(table) = open_table(path).map(|table| warm::warm_up(table, warm)) else {
        return;
    };
    shared.record(table.usage());
    lock(&shared.progress).is_loaded = true;
    shared.moved.notify_all();
    let mut writer = Writer::new(table);
    for job in inbox {
        match job {
            Job::Save {
                key,
                listing,
                is_forced,
            } => {
                let is_open = lock(&shared.open).contains(&key.location);
                if is_forced || is_open {
                    writer.write(&key, &listing, is_forced);
                    shared.record(writer.usage());
                }
                writer.forget_closed(&lock(&shared.open));
            }
            #[cfg(test)]
            Job::Stall(gate) => {
                let _ = gate.recv();
            }
        }
        lock(&shared.progress).finished += 1;
        shared.moved.notify_all();
    }
}

fn open_table(path: &Path) -> Option<ListingTable> {
    match CacheDb::open(path) {
        Ok((db, opened)) => {
            if let CacheOpen::Recreated { .. } = opened {
                tracing::warn!("listing cache was unreadable and was recreated");
            }
            Some(ListingTable::new(db, DISK_BUDGET))
        }
        Err(error) => {
            tracing::warn!(%error, "listing cache is unavailable");
            None
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
