use navo_compare::model::{ContentKey, HashCache};
use navo_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};
use navo_store::CacheDb;

use crate::compare::hash_cache::StoreHashCache;

const HASH: [u8; 32] = [5; 32];

fn key() -> ContentKey {
    ContentKey {
        provider: ProviderId::local(),
        full_path: ProviderPath::from_text("/work/a.txt"),
        size: 10,
        modified: Some(1_700_000_000_000),
        file_id: Some(FileIdentity {
            volume: 1,
            file: [2; 16],
            kind: FileIdentityKind::Native,
        }),
    }
}

fn cache() -> StoreHashCache {
    StoreHashCache::new(CacheDb::open_in_memory().unwrap())
}

#[test]
fn a_stored_hash_comes_back() {
    let cache = cache();
    cache.put(&key(), HASH);
    assert_eq!(cache.get(&key()), Some(HASH));
}

#[test]
fn a_changed_size_is_a_miss() {
    let cache = cache();
    cache.put(&key(), HASH);
    let changed = ContentKey { size: 11, ..key() };
    assert_eq!(cache.get(&changed), None);
}

#[test]
fn a_closed_store_misses_and_ignores_puts() {
    let cache = cache();
    cache.put(&key(), HASH);
    cache.close();
    assert_eq!(cache.get(&key()), None);
    cache.put(&key(), HASH);
    assert_eq!(cache.get(&key()), None);
}

#[test]
fn a_key_without_time_and_file_id_is_never_cached() {
    let cache = cache();
    let loose = ContentKey {
        modified: None,
        file_id: None,
        ..key()
    };
    cache.put(&loose, HASH);
    assert_eq!(cache.get(&loose), None);
}

#[test]
fn a_key_with_a_file_id_but_no_time_is_never_cached() {
    let cache = cache();
    let loose = ContentKey {
        modified: None,
        ..key()
    };
    cache.put(&loose, HASH);
    assert_eq!(cache.get(&loose), None);
}

#[test]
fn a_changed_modified_time_is_a_miss() {
    let cache = cache();
    cache.put(&key(), HASH);
    let edited = ContentKey {
        modified: Some(1_700_000_000_001),
        ..key()
    };
    assert_eq!(cache.get(&edited), None);
}
