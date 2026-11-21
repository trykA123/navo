//! Tests for the shared compare hash cache in `host_parts`.

use std::time::{SystemTime, UNIX_EPOCH};

use sidewise_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};
use sidewise_engine::model::{ContentKey, HashCache};

use super::{SharedHashes, open_hashes, shared_hashes};
use crate::compare_tab_tests::TempDir;

/// A settled local key for `name` (modified an hour ago, native identity).
fn key(name: &str) -> ContentKey {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    ContentKey {
        provider: ProviderId::local(),
        full_path: ProviderPath::from_text(&format!("/data/{name}")),
        size: 10,
        modified: Some(i64::try_from(now).unwrap() - 3_600_000),
        file_id: Some(FileIdentity {
            volume: 7,
            file: [name.len() as u8; 16],
            kind: FileIdentityKind::Native,
        }),
    }
}

#[test]
fn open_hashes_keeps_rows_in_the_profile_cache_file() {
    let dir = TempDir::new("w1-open");
    let key = key("kept");
    let first = open_hashes(Some(dir.path()));
    first.put(&key, [5; 32]);
    drop(first);
    assert!(dir.path().join("cache").join("hashes.v1").is_file());
    assert_eq!(open_hashes(Some(dir.path())).get(&key), Some([5; 32]));
}

#[test]
fn shared_hashes_without_a_profile_live_in_memory() {
    let key = key("memory-only");
    shared_hashes().put(&key, [6; 32]);
    assert_eq!(shared_hashes().get(&key), Some([6; 32]));
}

#[test]
fn shared_hashes_forwards_to_the_shared_store() {
    let key = key("forwarded-unique");
    SharedHashes.put(&key, [7; 32]);
    SharedHashes.flush();
    assert_eq!(shared_hashes().get(&key), Some([7; 32]));
    assert_eq!(SharedHashes.get(&key), Some([7; 32]));
}
