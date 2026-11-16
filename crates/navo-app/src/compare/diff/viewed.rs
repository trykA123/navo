//! Viewed marks over the store's `compare-viewed` table (D28).

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use navo_store::CacheError;

mod store;

#[cfg(test)]
pub(crate) use store::ViewedStore;
pub(crate) use store::opener;

/// A Viewed store failure, kept as its message so hosts need no store error type.
#[derive(Debug)]
pub(crate) struct ViewedError(pub String);

impl std::fmt::Display for ViewedError {
    /// Writes the stored message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<CacheError> for ViewedError {
    /// Keeps the store error's message.
    fn from(error: CacheError) -> Self {
        Self(error.to_string())
    }
}

/// Reads and writes Viewed marks; every call runs on a session worker.
pub(crate) trait ViewedMarks: Send + Sync {
    /// Whether this 64-byte content key is marked viewed.
    fn get(&self, key: &[u8; 64]) -> Result<bool, ViewedError>;
    /// Sets or clears the mark for this 64-byte content key.
    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), ViewedError>;
}

/// A store that never marks anything viewed, used when the cache cannot open.
pub(crate) struct NoViewed;

impl ViewedMarks for NoViewed {
    fn get(&self, _key: &[u8; 64]) -> Result<bool, ViewedError> {
        Ok(false)
    }

    fn set(&self, _key: &[u8; 64], _viewed: bool) -> Result<(), ViewedError> {
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
    fn get(&self, key: &[u8; 64]) -> Result<bool, ViewedError> {
        self.store().get(key)
    }

    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), ViewedError> {
        self.store().set(key, viewed)
    }
}

/// Viewed marks kept in memory only, for a host with no profile cache.
#[derive(Default)]
pub(crate) struct MemoryViewed(Mutex<HashSet<[u8; 64]>>);

impl ViewedMarks for MemoryViewed {
    fn get(&self, key: &[u8; 64]) -> Result<bool, ViewedError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(key))
    }

    fn set(&self, key: &[u8; 64], viewed: bool) -> Result<(), ViewedError> {
        let mut marks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if viewed {
            marks.insert(*key);
        } else {
            marks.remove(key);
        }
        Ok(())
    }
}
