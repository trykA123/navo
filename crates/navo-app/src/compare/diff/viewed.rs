//! Viewed marks over the store's `compare-viewed` table (D28).

use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_store::{CacheDb, CacheError};

/// Reads and writes Viewed marks; every call runs on a session worker.
pub(crate) trait ViewedMarks: Send + Sync {
    /// Whether this 64-byte content key is marked viewed.
    fn get(&self, key: &[u8; 64]) -> Result<bool, CacheError>;
    /// Sets or clears the mark for this 64-byte content key.
    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), CacheError>;
}

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
    fn get(&self, key: &[u8; 64]) -> Result<bool, CacheError> {
        let guard = self.db();
        match guard.as_ref() {
            Some(db) => db.get_viewed(key),
            None => Ok(false),
        }
    }

    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), CacheError> {
        let mut guard = self.db();
        let Some(db) = guard.as_mut() else {
            return Ok(());
        };
        if viewed {
            db.put_viewed(key, now_ms())
        } else {
            db.clear_viewed(key).map(|_| ())
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

/// A store that never marks anything viewed, used when the cache cannot open.
pub(crate) struct NoViewed;

impl ViewedMarks for NoViewed {
    fn get(&self, _key: &[u8; 64]) -> Result<bool, CacheError> {
        Ok(false)
    }

    fn set(&self, _key: &[u8; 64], _viewed: bool) -> Result<(), CacheError> {
        Ok(())
    }
}

/// Opens the real Viewed store; called once, on the first Stream worker that needs a mark.
pub(crate) type OpenViewed = Arc<dyn Fn() -> Arc<dyn ViewedMarks> + Send + Sync>;

/// A Viewed store opened on first use, so opening the cache never runs on the UI thread.
pub(crate) struct LazyViewed {
    open: OpenViewed,
    store: OnceLock<Arc<dyn ViewedMarks>>,
}

impl LazyViewed {
    /// Remember how to open the store; nothing is opened yet.
    pub(crate) fn new(open: OpenViewed) -> Self {
        Self {
            open,
            store: OnceLock::new(),
        }
    }

    /// The store, opened on the first call; other callers wait for that one open.
    fn store(&self) -> &Arc<dyn ViewedMarks> {
        self.store.get_or_init(|| (self.open)())
    }
}

impl ViewedMarks for LazyViewed {
    fn get(&self, key: &[u8; 64]) -> Result<bool, CacheError> {
        self.store().get(key)
    }

    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), CacheError> {
        self.store().set(key, viewed)
    }
}
