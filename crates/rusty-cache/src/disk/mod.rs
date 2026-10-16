pub(crate) mod codec;
mod table;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_store::{CacheDb, CacheOpen};

use crate::keys::Lookup;
use crate::{CacheKey, CacheKeys, CachedListing, ListingCache};
use codec::Body;
#[cfg(test)]
pub(crate) use codec::FORMAT;
pub(crate) use table::ListingTable;

pub const DISK_BUDGET: u64 = 32 * 1024 * 1024;

pub struct Warm {
    pub cache: ListingCache,
    pub keys: CacheKeys,
    pub folders: Vec<Location>,
    pub deadline: Instant,
}

enum Job {
    Save {
        key: CacheKey,
        listing: Arc<CachedListing>,
        is_forced: bool,
    },
    Flush(Sender<()>),
}

#[derive(Clone)]
pub struct DiskListings {
    jobs: Sender<Job>,
}

impl DiskListings {
    #[must_use]
    pub fn open(path: &Path, warm: Warm) -> Self {
        let (jobs, inbox) = mpsc::channel();
        let (loaded, ready) = mpsc::channel();
        let deadline = warm.deadline;
        let path = path.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("listing-disk".to_owned())
            .spawn(move || run(&path, warm, &loaded, inbox));
        match spawned {
            Ok(_) => {
                let _ = ready.recv_timeout(deadline.saturating_duration_since(Instant::now()));
            }
            Err(error) => tracing::warn!(%error, "listing cache writer did not start"),
        }
        Self { jobs }
    }

    pub fn save(&self, key: CacheKey, listing: Arc<CachedListing>) {
        self.send(key, listing, false);
    }

    pub fn save_folders(&self, cache: &ListingCache, keys: &CacheKeys, folders: &[Location]) {
        for location in folders {
            let Some(key) = keys.key(location) else {
                continue;
            };
            if let Some(listing) = cache.lock().peek(&key) {
                self.send(key, listing, true);
            }
        }
    }

    #[must_use]
    pub fn flush(&self, timeout: Duration) -> bool {
        let (done, flushed) = mpsc::channel();
        self.jobs.send(Job::Flush(done)).is_ok() && flushed.recv_timeout(timeout).is_ok()
    }

    fn send(&self, key: CacheKey, listing: Arc<CachedListing>, is_forced: bool) {
        let _ = self.jobs.send(Job::Save {
            key,
            listing,
            is_forced,
        });
    }
}

fn run(path: &Path, warm: Warm, loaded: &Sender<()>, inbox: Receiver<Job>) {
    let table = open_table(path).map(|table| warm_up(table, warm));
    let _ = loaded.send(());
    let Some(table) = table else {
        return;
    };
    let mut writer = Writer {
        table,
        written: HashMap::new(),
    };
    for job in inbox {
        match job {
            Job::Save {
                key,
                listing,
                is_forced,
            } => writer.write(&key, &listing, is_forced),
            Job::Flush(done) => {
                let _ = done.send(());
            }
        }
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

fn warm_up(mut table: ListingTable, warm: Warm) -> ListingTable {
    let Warm {
        cache,
        keys,
        mut folders,
        deadline,
    } = warm;
    for location in &folders {
        let _ = keys.key(location);
    }
    loop {
        let answers = keys.answers();
        folders.retain(|location| match keys.lookup(location) {
            Lookup::Found(key) => {
                admit(&mut table, &cache, key, deadline);
                false
            }
            Lookup::Probing => true,
            Lookup::Missing => false,
        });
        if folders.is_empty() || !keys.wait_for_answer(answers, deadline) {
            return table;
        }
    }
}

pub(crate) fn admit(
    table: &mut ListingTable,
    cache: &ListingCache,
    key: CacheKey,
    deadline: Instant,
) {
    if Instant::now() >= deadline {
        return;
    }
    let Some(mut listing) = table.load(&key) else {
        return;
    };
    listing.folder_changed = None;
    if Instant::now() < deadline {
        cache.put_absent(key, listing);
    }
}

struct Writer {
    table: ListingTable,
    written: HashMap<CacheKey, u32>,
}

impl Writer {
    fn write(&mut self, key: &CacheKey, listing: &CachedListing, is_forced: bool) {
        let body = Body::new(listing);
        let digest = body.rows_digest();
        if !is_forced && self.written.get(key) == Some(&digest) {
            return;
        }
        match self.table.save_body(key, &body) {
            Ok(()) => {
                self.written.insert(key.clone(), digest);
            }
            Err(error) => tracing::warn!(%error, "listing cache write failed"),
        }
    }
}
