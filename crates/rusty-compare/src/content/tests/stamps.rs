use rusty_core::{FileIdentity, FileIdentityKind};

use crate::content::read::content_key;
use crate::content::{CONTENT_BLOCK, ContentResult, check};
use crate::model::{CancelToken, ContentClass, Reason, Stamp, Status};

use super::fake::{FakeCache, assert_reads, entry, options, side, source};

#[test]
fn both_sides_truncated_to_the_same_shorter_content_is_changed_on_disk() {
    let listed = 2 * CONTENT_BLOCK;
    let shorter = CONTENT_BLOCK;
    let left_entry = entry("a.bin", u64::try_from(listed).unwrap());
    let right_entry = entry("a.bin", u64::try_from(listed).unwrap());
    let mut left_source = source("a.bin", vec![0xAA; listed]);
    let mut right_source = source("a.bin", vec![0xAA; listed]);
    left_source.set_bytes("a.bin", vec![0xAA; shorter]);
    right_source.set_bytes("a.bin", vec![0xAA; shorter]);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(
        result,
        ContentResult {
            status: Status::Unavailable(Reason::ChangedOnDisk),
            content: ContentClass::Unknown,
        }
    );
    let read = u64::try_from(shorter).unwrap();
    assert_reads(&left_stats, 1, read);
    assert_reads(&right_stats, 1, read);
    assert_eq!(cache.puts(), 0);
}

#[test]
fn both_sides_grown_identically_is_changed_on_disk() {
    let listed = CONTENT_BLOCK;
    let grown = CONTENT_BLOCK + 1;
    let left_entry = entry("a.bin", u64::try_from(listed).unwrap());
    let right_entry = entry("a.bin", u64::try_from(listed).unwrap());
    let mut left_source = source("a.bin", vec![0xBB; listed]);
    let mut right_source = source("a.bin", vec![0xBB; listed]);
    left_source.set_bytes("a.bin", vec![0xBB; grown]);
    right_source.set_bytes("a.bin", vec![0xBB; grown]);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(result.status, Status::Unavailable(Reason::ChangedOnDisk));
    let read = u64::try_from(grown).unwrap();
    assert_reads(&left_stats, 1, read);
    assert_reads(&right_stats, 1, read);
    assert_eq!(cache.puts(), 0);
}

#[test]
fn a_one_sided_cache_hit_with_a_truncated_side_is_changed_on_disk() {
    let listed = 2 * CONTENT_BLOCK;
    let shorter = CONTENT_BLOCK;
    let bytes = vec![0xCC; listed];
    let left_entry = entry("a.bin", u64::try_from(listed).unwrap());
    let right_entry = entry("a.bin", u64::try_from(listed).unwrap());
    let left_source = source("a.bin", bytes.clone()).with_root("left");
    let mut right_source = source("a.bin", bytes).with_root("right");
    right_source.set_bytes("a.bin", vec![0xCC; shorter]);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let hash = *blake3::hash(&vec![0xCC; listed]).as_bytes();
    let cache = FakeCache::with(content_key(&left_source, &left_entry), hash);
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(result.status, Status::Unavailable(Reason::ChangedOnDisk));
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 1, u64::try_from(shorter).unwrap());
    assert_eq!(cache.puts(), 0);
}

#[test]
fn an_mtime_change_before_the_read_is_changed_on_disk() {
    let size = 4;
    let mut left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let mut right_entry = entry("a.bin", u64::try_from(size).unwrap());
    left_entry.modified = Some(1);
    right_entry.modified = Some(1);
    let mut left_source = source("a.bin", b"data".to_vec());
    let mut right_source = source("a.bin", b"data".to_vec());
    let stamp = Stamp {
        size: u64::try_from(size).unwrap(),
        modified: Some(2),
        file_id: None,
    };
    left_source.set_stamp("a.bin", stamp);
    right_source.set_stamp("a.bin", stamp);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(result.status, Status::Unavailable(Reason::ChangedOnDisk));
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 0, 0);
    assert_eq!(cache.gets(), 0);
    assert_eq!(cache.puts(), 0);
}

#[test]
fn a_same_size_new_mtime_rewrite_does_not_trust_the_cache() {
    let identity = FileIdentity {
        volume: 7,
        file: [0x33; 16],
        kind: FileIdentityKind::Native,
    };
    let size = 4;
    let mut left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let mut right_entry = entry("a.bin", u64::try_from(size).unwrap());
    left_entry.modified = Some(1);
    right_entry.modified = Some(1);
    let mut left_source = source("a.bin", b"data".to_vec()).with_root("left");
    let mut right_source = source("a.bin", b"data".to_vec()).with_root("right");
    let stamp = Stamp {
        size: u64::try_from(size).unwrap(),
        modified: Some(2),
        file_id: Some(identity),
    };
    left_source.set_stamp("a.bin", stamp);
    right_source.set_stamp("a.bin", stamp);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let hash = *blake3::hash(b"data").as_bytes();
    cache.seed(content_key(&left_source, &left_entry), hash);
    cache.seed(content_key(&right_source, &right_entry), hash);
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(result.status, Status::Unavailable(Reason::ChangedOnDisk));
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 0, 0);
    assert_eq!(cache.gets(), 0);
    assert_eq!(cache.puts(), 0);
}
