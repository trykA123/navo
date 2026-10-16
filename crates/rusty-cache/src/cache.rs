use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::store::Store;
use crate::{CacheKey, CachedListing, Change};

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
        }
    }

    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<Arc<CachedListing>> {
        self.lock().get(key)
    }

    pub fn put(&self, key: CacheKey, listing: CachedListing) {
        self.lock().put(key, listing);
    }

    pub fn patch(&self, key: &CacheKey, changes: impl IntoIterator<Item = Change>) {
        self.lock().patch(key, changes);
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

    fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
