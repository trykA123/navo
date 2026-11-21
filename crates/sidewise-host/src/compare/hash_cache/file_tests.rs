//! Tests for the file-backed hash cache: open, lock, replay, the open-time rewrite, put and flush.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_compare::model::{ContentKey, HashCache};
use navo_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};

use super::file::{CacheMode, FileHashes, Limits};
use super::record::{MAGIC, encode, key_digest};

/// A fixed, settled modified time (2023-11-14): a key built twice must digest the same.
const SETTLED_MS: i64 = 1_700_000_000_000;

/// A local key for `name`, modified at `SETTLED_MS`, with a native identity.
fn key(name: &str) -> ContentKey {
    key_at(name, SETTLED_MS)
}

/// A local key for `name`, modified `offset_ms` from now (negative is the past).
fn key_aged(name: &str, offset_ms: i64) -> ContentKey {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    key_at(name, i64::try_from(now).unwrap() + offset_ms)
}

/// A local key for `name` modified at `modified_ms`, with a native identity.
fn key_at(name: &str, modified_ms: i64) -> ContentKey {
    ContentKey {
        provider: ProviderId::local(),
        full_path: ProviderPath::from_text(&format!("/data/{name}")),
        size: 10,
        modified: Some(modified_ms),
        file_id: Some(FileIdentity {
            volume: 7,
            file: [name.len() as u8; 16],
            kind: FileIdentityKind::Native,
        }),
    }
}

/// The cache path under `sub`, whose folder does not exist yet.
fn cache_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("sub").join("hashes.v1")
}

/// One encoded record for `key` and `hash`.
fn row(key: &ContentKey, hash: [u8; 32]) -> Vec<u8> {
    encode(&key_digest(key).unwrap(), &hash).to_vec()
}

/// Writes `bytes` to `path`, creating its folder.
fn write_file(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// True when the `.tmp` sibling of `path` exists.
fn temp_exists(path: &Path) -> bool {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name).exists()
}

#[test]
fn hashes_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let (a, b) = (key("a"), key("bb"));
    let bytes = [
        MAGIC.to_vec(),
        row(&a, [1; 32]),
        row(&b, [2; 32]),
        row(&a, [3; 32]),
    ]
    .concat();
    write_file(&path, &bytes);
    for _ in 0..2 {
        let store = FileHashes::open(&path);
        assert_eq!(store.mode(), CacheMode::Writer);
        assert_eq!(store.get(&a), Some([3; 32]), "later records win");
        assert_eq!(store.get(&b), Some([2; 32]));
        assert_eq!(store.get(&key("ccc")), None);
        assert_eq!(store.len(), 2);
    }
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "a clean file is not rewritten"
    );
}

#[test]
fn a_second_store_on_the_same_file_only_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let a = key("a");
    write_file(&path, &[MAGIC.to_vec(), row(&a, [1; 32])].concat());
    let first = FileHashes::open(&path);
    assert_eq!(first.mode(), CacheMode::Writer);
    let torn = [MAGIC.to_vec(), row(&a, [1; 32]), vec![9; 10]].concat();
    fs::write(&path, &torn).unwrap();
    let second = FileHashes::open(&path);
    assert_eq!(second.mode(), CacheMode::Reader);
    assert_eq!(second.get(&a), Some([1; 32]));
    assert_eq!(fs::read(&path).unwrap(), torn, "a reader never rewrites");
    drop(second);
    drop(first);
    let third = FileHashes::open(&path);
    assert_eq!(third.mode(), CacheMode::Writer, "the lock is freed on drop");
}

#[test]
fn a_torn_tail_is_rewritten_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let (a, b) = (key("a"), key("bb"));
    let mut bad = row(&b, [2; 32]);
    bad[40] ^= 1;
    write_file(
        &path,
        &[MAGIC.to_vec(), row(&a, [1; 32]), bad, vec![9; 10]].concat(),
    );
    let store = FileHashes::open(&path);
    assert_eq!(store.mode(), CacheMode::Writer);
    assert_eq!(store.get(&a), Some([1; 32]));
    assert_eq!(store.get(&b), None);
    assert_eq!(
        fs::read(&path).unwrap(),
        [MAGIC.to_vec(), row(&a, [1; 32])].concat()
    );
    assert!(!temp_exists(&path));
}

#[test]
fn a_bad_header_starts_empty_and_rewrites() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let a = key("a");
    write_file(
        &path,
        &[b"SWHASH\x00\x02".to_vec(), row(&a, [1; 32])].concat(),
    );
    let store = FileHashes::open(&path);
    assert!(store.is_empty());
    assert_eq!(store.get(&a), None);
    assert_eq!(fs::read(&path).unwrap(), MAGIC.to_vec());
}

#[test]
fn a_failed_rewrite_removes_the_temp_and_stops_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    fs::create_dir_all(&path).unwrap();
    let store = FileHashes::open(&path);
    assert_eq!(
        store.mode(),
        CacheMode::Reader,
        "a failed rewrite drops the writer"
    );
    assert!(store.is_empty());
    assert!(path.is_dir());
    assert!(!temp_exists(&path));
}

#[test]
fn memory_mode_keeps_no_file() {
    let store = FileHashes::memory();
    assert_eq!(store.mode(), CacheMode::Memory);
    assert!(store.is_empty());
    assert_eq!(store.get(&key("a")), None);
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("plain-file");
    fs::write(&blocker, b"x").unwrap();
    let path = blocker.join("hashes.v1");
    let store = FileHashes::open(&path);
    assert_eq!(
        store.mode(),
        CacheMode::Memory,
        "no parent folder means memory mode"
    );
    assert!(!path.exists());
}

#[test]
fn put_then_reopen_keeps_the_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let (a, b) = (key("a"), key("bb"));
    let store = FileHashes::open(&path);
    store.put(&a, [1; 32]);
    store.put(&b, [2; 32]);
    store.put(&a, [3; 32]);
    assert_eq!(store.get(&a), Some([3; 32]));
    store.flush();
    let expected = [
        MAGIC.to_vec(),
        row(&a, [1; 32]),
        row(&b, [2; 32]),
        row(&a, [3; 32]),
    ]
    .concat();
    assert_eq!(fs::read(&path).unwrap(), expected);
    drop(store);
    let store = FileHashes::open(&path);
    assert_eq!(store.get(&a), Some([3; 32]));
    assert_eq!(store.get(&b), Some([2; 32]));
    assert_eq!(fs::read(&path).unwrap(), expected, "a reopen keeps the log");
}

#[test]
fn a_recent_mtime_is_not_stored() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let store = FileHashes::open(&path);
    let (recent, old) = (key_aged("a", -1_000), key("bb"));
    store.put(&recent, [1; 32]);
    store.put(&old, [2; 32]);
    assert_eq!(store.get(&recent), None);
    assert_eq!(store.len(), 1, "only the old row is stored");
    drop(store);
    assert_eq!(
        fs::read(&path).unwrap(),
        [MAGIC.to_vec(), row(&old, [2; 32])].concat()
    );
}

#[test]
fn a_future_mtime_is_not_stored() {
    let store = FileHashes::memory();
    let (future, old) = (key_aged("a", 60_000), key("bb"));
    store.put(&future, [1; 32]);
    store.put(&old, [2; 32]);
    assert_eq!(store.get(&future), None);
    assert_eq!(store.len(), 1, "only the old row is stored");
}

#[test]
fn a_reader_puts_stay_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let first = FileHashes::open(&path);
    let second = FileHashes::open(&path);
    assert_eq!(second.mode(), CacheMode::Reader);
    let a = key("a");
    second.put(&a, [1; 32]);
    assert_eq!(second.get(&a), Some([1; 32]));
    second.flush();
    drop(second);
    assert!(!path.exists(), "a reader writes nothing");
    assert_eq!(first.get(&a), None);
    let memory = FileHashes::memory();
    memory.put(&a, [2; 32]);
    assert_eq!(memory.get(&a), Some([2; 32]));
}

#[test]
fn drop_flushes_pending_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let a = key("a");
    let store = FileHashes::open(&path);
    store.put(&a, [1; 32]);
    assert!(!path.exists(), "puts are buffered");
    drop(store);
    assert_eq!(
        fs::read(&path).unwrap(),
        [MAGIC.to_vec(), row(&a, [1; 32])].concat()
    );
}

#[test]
fn an_equal_put_appends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let a = key("a");
    let store = FileHashes::open(&path);
    store.put(&a, [1; 32]);
    store.flush();
    let length = fs::metadata(&path).unwrap().len();
    store.put(&a, [1; 32]);
    store.flush();
    drop(store);
    assert_eq!(fs::metadata(&path).unwrap().len(), length);
}

#[test]
fn the_1024th_put_appends_without_a_flush() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let store = FileHashes::open(&path);
    let a = key("a");
    for n in 0..1023u32 {
        let mut hash = [1; 32];
        hash[..4].copy_from_slice(&n.to_le_bytes());
        store.put(&a, hash);
    }
    assert!(!path.exists(), "1,023 records stay buffered");
    store.put(&a, [9; 32]);
    assert_eq!(fs::metadata(&path).unwrap().len(), 8 + 1024 * 72);
}

#[test]
fn a_failed_append_stops_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let store = FileHashes::open(&path);
    assert_eq!(store.mode(), CacheMode::Writer);
    fs::create_dir_all(&path).unwrap();
    let a = key("a");
    store.put(&a, [1; 32]);
    store.flush();
    assert_eq!(store.mode(), CacheMode::Reader, "the writer is dropped");
    assert_eq!(store.get(&a), Some([1; 32]), "rows stay in memory");
    fs::remove_dir(&path).unwrap();
    store.put(&key("bb"), [2; 32]);
    drop(store);
    assert!(!path.exists(), "a failed writer never writes again");
}

/// Limits small enough to test: keep 4 rows at open, refuse new keys at 5.
const SMALL: Limits = Limits {
    cap_rows: 4,
    ceiling_rows: 5,
};

#[test]
fn over_the_cap_keeps_the_newest_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let keys: Vec<_> = (1..=6).map(|n| key(&"k".repeat(n))).collect();
    let mut bytes = vec![MAGIC.to_vec()];
    bytes.extend(keys.iter().map(|k| row(k, [1; 32])));
    bytes.push(row(&keys[0], [9; 32]));
    write_file(&path, &bytes.concat());
    let store = FileHashes::open_with(&path, SMALL);
    assert_eq!(store.mode(), CacheMode::Writer);
    assert_eq!(store.len(), 4);
    assert_eq!(store.get(&keys[0]), Some([9; 32]), "a rewritten key is new");
    assert_eq!(store.get(&keys[1]), None);
    assert_eq!(store.get(&keys[2]), None);
    let mut kept = vec![MAGIC.to_vec()];
    kept.extend(keys[3..].iter().map(|k| row(k, [1; 32])));
    kept.push(row(&keys[0], [9; 32]));
    assert_eq!(fs::read(&path).unwrap(), kept.concat());
    assert!(!temp_exists(&path));
}

#[test]
fn the_ceiling_stops_new_keys_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let keys: Vec<_> = (1..=6).map(|n| key(&"k".repeat(n))).collect();
    let store = FileHashes::open_with(&path, SMALL);
    for k in &keys {
        store.put(k, [1; 32]);
    }
    assert_eq!(store.len(), 5);
    assert_eq!(store.get(&keys[5]), None, "the sixth key is refused");
    store.put(&keys[0], [2; 32]);
    assert_eq!(
        store.get(&keys[0]),
        Some([2; 32]),
        "a known key still updates"
    );
    store.flush();
    let mut expected = vec![MAGIC.to_vec()];
    expected.extend(keys[..5].iter().map(|k| row(k, [1; 32])));
    expected.push(row(&keys[0], [2; 32]));
    assert_eq!(fs::read(&path).unwrap(), expected.concat());
}

#[test]
fn a_log_of_stale_repeats_is_compacted() {
    let dir = tempfile::tempdir().unwrap();
    let path = cache_path(&dir);
    let (a, b) = (key("a"), key("bb"));
    let repeats = |count: usize| {
        let mut bytes = vec![MAGIC.to_vec(), row(&b, [7; 32])];
        bytes.extend((0..count).map(|n| row(&a, [(n % 200) as u8; 32])));
        bytes.concat()
    };
    // 2 live rows allow 2 * 2 + 4096 = 4100 records: one under the limit, then one over.
    let at_limit = repeats(4099);
    write_file(&path, &at_limit);
    drop(FileHashes::open(&path));
    assert_eq!(
        fs::read(&path).unwrap(),
        at_limit,
        "at the limit the log stays"
    );
    write_file(&path, &repeats(4100));
    let store = FileHashes::open(&path);
    assert_eq!(store.get(&a), Some([(4099 % 200) as u8; 32]));
    assert_eq!(store.get(&b), Some([7; 32]));
    let compact = [
        MAGIC.to_vec(),
        row(&b, [7; 32]),
        row(&a, [(4099 % 200) as u8; 32]),
    ];
    assert_eq!(fs::read(&path).unwrap(), compact.concat());
}
