//! Cache wrappers a compare session picks: none for network sides, recheck for `F5`.
//! Sits between the shared hash store and the engine session.

use std::sync::Arc;

use navo_compare::model::{ContentKey, HashCache};

/// A cache that misses every lookup and stores nothing.
pub struct NoHashCache;

impl HashCache for NoHashCache {
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &ContentKey, _hash: [u8; 32]) {}
}

/// A cache that never answers a lookup but stores every hash, so a recheck refreshes it.
pub struct RecheckHashes(pub Arc<dyn HashCache>);

impl HashCache for RecheckHashes {
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, key: &ContentKey, hash: [u8; 32]) {
        self.0.put(key, hash);
    }

    fn flush(&self) {
        self.0.flush();
    }
}

/// Picks the cache for one session: none when a side is on a network, else `cache`.
pub fn session_cache(cache: Arc<dyn HashCache>, network: bool) -> Arc<dyn HashCache> {
    if network {
        Arc::new(NoHashCache)
    } else {
        cache
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use navo_core::{ProviderId, ProviderPath};

    use super::*;

    /// Remembers every stored hash.
    #[derive(Default)]
    struct Store(Mutex<Vec<[u8; 32]>>);

    impl HashCache for Store {
        fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
            self.0.lock().unwrap().first().copied()
        }

        fn put(&self, _key: &ContentKey, hash: [u8; 32]) {
            self.0.lock().unwrap().push(hash);
        }
    }

    fn key() -> ContentKey {
        ContentKey {
            provider: ProviderId::local(),
            full_path: ProviderPath::from_text("/a"),
            size: 1,
            modified: None,
            file_id: None,
        }
    }

    #[test]
    fn network_sides_get_no_cache() {
        let store = Arc::new(Store::default());
        store.put(&key(), [1; 32]);
        let cache = session_cache(store.clone(), true);
        cache.put(&key(), [2; 32]);
        assert_eq!(cache.get(&key()), None);
        assert_eq!(store.0.lock().unwrap().len(), 1);
        assert!(session_cache(store, false).get(&key()).is_some());
    }

    #[test]
    fn recheck_misses_but_stores() {
        let store = Arc::new(Store::default());
        store.put(&key(), [1; 32]);
        let cache = RecheckHashes(store.clone());
        assert_eq!(cache.get(&key()), None);
        cache.put(&key(), [2; 32]);
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }
}
