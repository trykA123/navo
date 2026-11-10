// Viewed marks for compare pairs, keyed by the BLAKE3 of both sides' raw bytes.
// The cache keeps them across restarts and evicts the oldest when it is full.

use super::{CacheDb, CacheError};

pub const COMPARE_VIEWED_BUDGET: u64 = 4 * 1024 * 1024;

pub(super) const TABLE: &str = "compare-viewed";

impl CacheDb {
    /// Tells whether the pair with this 64-byte content key was marked viewed.
    pub fn get_viewed(&self, key: &[u8; 64]) -> Result<bool, CacheError> {
        Ok(self.get(TABLE, key)?.is_some())
    }

    /// Marks the pair as viewed and stores when it was set, in milliseconds since the epoch.
    pub fn put_viewed(&mut self, key: &[u8; 64], set_ms: i64) -> Result<(), CacheError> {
        self.put(TABLE, key, &set_ms.to_le_bytes())
    }

    /// Removes the viewed mark of this pair; returns true when one was stored.
    pub fn clear_viewed(&mut self, key: &[u8; 64]) -> Result<bool, CacheError> {
        self.remove(TABLE, key)
    }
}

/// Registers the table with its 4 MiB budget when the cache opens.
pub(super) fn configure(cache: &mut CacheDb) {
    cache.set_budget(TABLE, COMPARE_VIEWED_BUDGET);
}
