//! The cache-backed Viewed store and its opener (D28); Navo's only Viewed backend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_store::CacheDb;

use super::{NoViewed, OpenViewed, ViewedError, ViewedMarks};

/// The `compare-viewed` table behind a mutex, so writes never reach the UI thread.
pub(crate) struct ViewedStore {
    db: Mutex<Option<CacheDb>>,
}

impl ViewedStore {
    /// Keep this open cache as the Viewed store.
    pub(crate) fn new(db: CacheDb) -> Self {
        Self {
            db: Mutex::new(Some(db)),
        }
    }

    fn db(&self) -> MutexGuard<'_, Option<CacheDb>> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ViewedMarks for ViewedStore {
    fn get(&self, key: &[u8; 64]) -> Result<bool, ViewedError> {
        let guard = self.db();
        match guard.as_ref() {
            Some(db) => db.get_viewed(key).map_err(ViewedError::from),
            None => Ok(false),
        }
    }

    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), ViewedError> {
        let mut guard = self.db();
        let Some(db) = guard.as_mut() else {
            return Ok(());
        };
        if viewed {
            db.put_viewed(key, now_ms()).map_err(ViewedError::from)
        } else {
            db.clear_viewed(key).map(|_| ()).map_err(ViewedError::from)
        }
    }
}

/// The current time in milliseconds, or zero before the epoch.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// An opener for the Viewed store: the cache at `cache_db` (its folder is created), else memory.
pub(crate) fn opener(cache_db: Option<PathBuf>) -> OpenViewed {
    Arc::new(move || open_viewed(cache_db.as_deref()))
}

/// Open the cache file for Viewed marks, or fall back to memory and then a no-op store.
fn open_viewed(cache_db: Option<&Path>) -> Arc<dyn ViewedMarks> {
    let db = cache_db
        .filter(|path| {
            path.parent()
                .is_none_or(|dir| fs::create_dir_all(dir).is_ok())
        })
        .and_then(|path| CacheDb::open(path).ok())
        .map(|(db, _)| db);
    match db.or_else(|| CacheDb::open_in_memory().ok()) {
        Some(db) => Arc::new(ViewedStore::new(db)),
        None => Arc::new(NoViewed),
    }
}
