pub(crate) mod codec;
mod table;
mod warm;
mod writer;

use std::collections::HashSet;
use std::path::Path;
#[cfg(test)]
use std::sync::mpsc::Sender;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
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

#[derive(Default)]
struct Shared {
    open: Mutex<HashSet<Location>>,
    progress: Mutex<Progress>,
    moved: Condvar,
}

#[derive(Clone)]
pub struct DiskListings {
    jobs: SyncSender<Job>,
    shared: Arc<Shared>,
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
        match spawned {
            Ok(_) => {
                let left = deadline.saturating_duration_since(Instant::now());
                let progress = lock(&shared.progress);
                let _ = shared
                    .moved
                    .wait_timeout_while(progress, left, |progress| {
                        !progress.is_loaded && !progress.is_gone
                    })
                    .unwrap_or_else(PoisonError::into_inner);
            }
            Err(error) => {
                tracing::warn!(%error, "listing cache writer did not start");
                lock(&shared.progress).is_gone = true;
            }
        }
        Self { jobs, shared }
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
        let mut progress = lock(&self.shared.progress);
        if self.jobs.try_send(job).is_ok() {
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
    let table = open_table(path).map(|table| warm::warm_up(table, warm));
    lock(&shared.progress).is_loaded = true;
    shared.moved.notify_all();
    let Some(table) = table else {
        return;
    };
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
