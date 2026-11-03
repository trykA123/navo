//! The folder-compare hash cache over the store's `content-hash` table.

use std::sync::{Mutex, MutexGuard, PoisonError};

use navo_compare::model::{ContentKey, HashCache};
use navo_store::{CacheDb, ContentKey as StoreKey};

/// A store failure, a closed store or a key without a modified time is a miss.
pub(crate) struct StoreHashCache {
    db: Mutex<Option<CacheDb>>,
}

impl StoreHashCache {
    pub(crate) fn new(db: CacheDb) -> Self {
        Self {
            db: Mutex::new(Some(db)),
        }
    }

    /// Drops the store; later calls miss.
    pub(crate) fn close(&self) {
        self.db().take();
    }

    fn db(&self) -> MutexGuard<'_, Option<CacheDb>> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl HashCache for StoreHashCache {
    fn get(&self, key: &ContentKey) -> Option<[u8; 32]> {
        let key = store_key(key)?;
        let guard = self.db();
        let found = guard.as_ref()?.get_content_hash(&key);
        match found {
            Ok(hash) => hash,
            Err(error) => {
                tracing::debug!(%error, "compare hash cache read failed");
                None
            }
        }
    }

    fn put(&self, key: &ContentKey, hash: [u8; 32]) {
        let Some(key) = store_key(key) else {
            return;
        };
        let mut guard = self.db();
        let Some(db) = guard.as_mut() else {
            return;
        };
        if let Err(error) = db.put_content_hash(&key, hash) {
            tracing::debug!(%error, "compare hash cache write failed");
        }
    }
}

/// Without a modified time nothing proves the bytes are unchanged, even with a file id.
fn store_key(key: &ContentKey) -> Option<StoreKey> {
    key.modified?;
    Some(StoreKey {
        provider: key.provider.clone(),
        path: key.full_path.clone(),
        size: key.size,
        modified: key.modified,
        file_id: key.file_id,
    })
}
