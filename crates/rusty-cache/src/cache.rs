use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use crate::store::{SizedListing, Store, Swap};
use crate::{CacheKey, CachedListing, Change, DiskListings};

pub const DEFAULT_BUDGET: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub bytes: usize,
    pub entries: usize,
    pub budget: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

#[derive(Clone)]
pub struct ListingCache {
    store: Arc<Mutex<Store>>,
    disk: Option<DiskListings>,
}

impl Default for ListingCache {
    fn default() -> Self {
        Self::new(DEFAULT_BUDGET)
    }
}

impl ListingCache {
    #[must_use]
    pub fn new(budget: usize) -> Self {
        Self {
            store: Arc::new(Mutex::new(Store::new(budget))),
            disk: None,
        }
    }

    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<Arc<CachedListing>> {
        self.lock().get(key)
    }

    pub fn put(&self, key: CacheKey, listing: CachedListing) {
        let entry = SizedListing::new(&key, listing);
        self.lock().put(key, entry);
    }

    pub fn patch(
        &self,
        key: &CacheKey,
        changed_at: SystemTime,
        changes: impl IntoIterator<Item = Change>,
    ) {
        let changes: Vec<Change> = changes.into_iter().collect();
        for _ in 0..2 {
            let Some(seen) = self.lock().peek(key) else {
                return;
            };
            if seen.read_at > changed_at {
                return;
            }
            let entry = SizedListing::new(key, seen.patched(&changes, changed_at));
            if self.lock().swap(key, &seen, entry) != Swap::Stale {
                return;
            }
        }
        self.lock().invalidate(key);
    }

    #[must_use]
    pub fn with_disk(self, disk: DiskListings) -> Self {
        Self {
            disk: Some(disk),
            ..self
        }
    }

    pub(crate) fn put_absent(&self, key: CacheKey, listing: CachedListing) {
        let entry = SizedListing::new(&key, listing);
        let mut store = self.lock();
        if store.peek(&key).is_none() {
            store.put(key, entry);
        }
    }

    pub fn persist(&self, key: &CacheKey) {
        let Some(disk) = &self.disk else {
            return;
        };
        if let Some(listing) = self.lock().peek(key) {
            disk.save(key.clone(), listing);
        }
    }

    pub fn invalidate(&self, key: &CacheKey) {
        self.lock().invalidate(key);
    }

    pub fn pin(&self, key: &CacheKey) {
        self.lock().pin(key);
    }

    pub fn unpin(&self, key: &CacheKey) {
        self.lock().unpin(key);
    }

    #[must_use]
    pub fn stats(&self) -> CacheStats {
        self.lock().stats()
    }

    #[must_use]
    pub fn pinned_bytes(&self) -> usize {
        self.lock().pinned_bytes()
    }

    #[cfg(test)]
    pub(crate) fn scanned_pinned_bytes(&self) -> usize {
        self.lock().scanned_pinned_bytes()
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
