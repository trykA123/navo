//! Tests for clearing the hash cache: the writer removes the file, a reader refuses.

use std::fs;

use navo_compare::model::{ContentKey, HashCache};
use navo_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};

use super::file::{CacheMode, ClearError, FileHashes};
use super::record::MAGIC;

/// A local key for `name`, modified at a fixed settled time (2023-11-14), with a native
/// identity; a fixed time keeps the digest equal when the key is built twice.
fn key(name: &str) -> ContentKey {
    ContentKey {
        provider: ProviderId::local(),
        full_path: ProviderPath::from_text(&format!("/data/{name}")),
        size: 10,
        modified: Some(1_700_000_000_000),
        file_id: Some(FileIdentity {
            volume: 7,
            file: [name.len() as u8; 16],
            kind: FileIdentityKind::Native,
        }),
    }
}

#[test]
fn clear_removes_the_file_and_the_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hashes.v1");
    let store = FileHashes::open(&path);
    store.put(&key("a"), [1; 32]);
    store.put(&key("bb"), [2; 32]);
    store.flush();
    assert!(path.exists());
    assert_eq!(store.clear_all().unwrap(), 2);
    assert!(store.is_empty());
    assert!(!path.exists());
    assert_eq!(store.get(&key("a")), None);
}

#[test]
fn a_reader_cannot_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hashes.v1");
    let writer = FileHashes::open(&path);
    writer.put(&key("a"), [1; 32]);
    writer.flush();
    let reader = FileHashes::open(&path);
    assert_eq!(reader.mode(), CacheMode::Reader);
    assert_eq!(reader.clear_all(), Err(ClearError::Busy));
    assert_eq!(reader.len(), 1);
    assert!(path.exists());
}

#[test]
fn a_failed_writer_clears_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hashes.v1");
    fs::create_dir_all(&path).unwrap();
    let store = FileHashes::open(&path);
    assert_eq!(store.mode(), CacheMode::Reader);
    store.put(&key("a"), [1; 32]);
    assert_eq!(store.clear_all().unwrap(), 1);
    assert!(store.is_empty());
    assert!(path.is_dir(), "the file is not touched");
}

#[test]
fn puts_after_clear_start_a_fresh_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hashes.v1");
    let store = FileHashes::open(&path);
    store.put(&key("a"), [1; 32]);
    store.flush();
    store.clear_all().unwrap();
    store.put(&key("bb"), [2; 32]);
    store.flush();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[..MAGIC.len()], &MAGIC);
    drop(store);
    let again = FileHashes::open(&path);
    assert_eq!(again.len(), 1);
    assert_eq!(again.get(&key("bb")), Some([2; 32]));
    assert_eq!(again.get(&key("a")), None);
}
