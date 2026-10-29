use rusty_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};

use super::*;

const HASH: [u8; 32] = [7; 32];
const MODIFIED: i64 = 1_700_000_000_000;
const EVICTION_KEYS: usize = 300;
const EVICTION_PATH_BYTES: usize = 64 * 1024;

fn provider() -> ProviderId {
    ProviderId::new("local").unwrap()
}

fn path(text: &str) -> ProviderPath {
    ProviderPath::from_text(text)
}

fn identity() -> FileIdentity {
    FileIdentity {
        volume: 3,
        file: [9; 16],
        kind: FileIdentityKind::Native,
    }
}

fn key() -> ContentKey {
    ContentKey {
        provider: provider(),
        path: path(r"C:\work\a.txt"),
        size: 42,
        modified: Some(MODIFIED),
        file_id: Some(identity()),
    }
}

#[test]
fn round_trip_returns_the_hash() {
    let mut cache = mem();
    cache.put_content_hash(&key(), HASH).unwrap();
    assert_eq!(cache.get_content_hash(&key()).unwrap(), Some(HASH));
}

#[test]
fn round_trip_with_absent_optional_fields() {
    let mut cache = mem();
    let key = ContentKey {
        modified: None,
        file_id: None,
        ..key()
    };
    cache.put_content_hash(&key, HASH).unwrap();
    assert_eq!(cache.get_content_hash(&key).unwrap(), Some(HASH));
}

#[test]
fn a_changed_size_misses() {
    let mut cache = mem();
    let original = key();
    cache.put_content_hash(&original, HASH).unwrap();
    let changed = ContentKey {
        size: original.size + 1,
        ..original
    };
    assert_eq!(cache.get_content_hash(&changed).unwrap(), None);
}

#[test]
fn a_changed_modified_time_misses() {
    let mut cache = mem();
    let original = key();
    cache.put_content_hash(&original, HASH).unwrap();
    let changed = ContentKey {
        modified: Some(MODIFIED + 1),
        ..original.clone()
    };
    assert_eq!(cache.get_content_hash(&changed).unwrap(), None);
    let changed = ContentKey {
        modified: None,
        ..original
    };
    assert_eq!(cache.get_content_hash(&changed).unwrap(), None);
}

#[test]
fn a_changed_file_id_misses() {
    let mut cache = mem();
    let original = key();
    cache.put_content_hash(&original, HASH).unwrap();
    let mut changed = original.clone();
    changed.file_id.as_mut().unwrap().file[0] = 8;
    assert_eq!(cache.get_content_hash(&changed).unwrap(), None);
    let mut changed = original.clone();
    changed.file_id.as_mut().unwrap().kind = FileIdentityKind::WindowsIndex;
    assert_eq!(cache.get_content_hash(&changed).unwrap(), None);
    let changed = ContentKey {
        file_id: None,
        ..original
    };
    assert_eq!(cache.get_content_hash(&changed).unwrap(), None);
}

#[test]
fn budget_eviction_keeps_the_table_under_16_mib() {
    let mut cache = mem();
    let prefix = "p".repeat(EVICTION_PATH_BYTES);
    let mut first = None;
    let mut last = None;
    for index in 0..EVICTION_KEYS {
        let key = ContentKey {
            path: path(&format!("{prefix}{index:04}")),
            ..key()
        };
        let hash = [index as u8; 32];
        cache.put_content_hash(&key, hash).unwrap();
        if first.is_none() {
            first = Some(key.clone());
        }
        last = Some((key, hash));
    }
    let (used, entries) = cache.usage("content-hash").unwrap();
    assert!(used <= CONTENT_HASH_BUDGET);
    assert!(entries < EVICTION_KEYS as u64);
    assert_eq!(
        cache.get_content_hash(first.as_ref().unwrap()).unwrap(),
        None
    );
    let (last_key, last_hash) = last.as_ref().unwrap();
    assert_eq!(cache.get_content_hash(last_key).unwrap(), Some(*last_hash));
}
