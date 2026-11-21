use std::time::{SystemTime, UNIX_EPOCH};

use navo_core::{FileIdentity, FileIdentityKind};

use crate::content::read::content_key;
use crate::content::{CONTENT_BLOCK, ContentResult, check};
use crate::model::{CancelToken, ContentClass, Reason, SideEntry, Stamp, Status};

use super::fake::{FakeCache, FakeSource, assert_reads, entry, options, side, source};

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

#[test]
fn two_cached_hashes_that_disagree_are_verified_by_reading() {
    let size = 2 * CONTENT_BLOCK;
    let bytes = vec![0x5A; size];
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", bytes.clone()).with_root("left");
    let right_source = source("a.bin", bytes).with_root("right");
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::with(content_key(&left_source, &left_entry), [1; 32]);
    cache.seed(content_key(&right_source, &right_entry), [2; 32]);
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(result.status, Status::Same);
    let read = u64::try_from(size).unwrap();
    assert_reads(&left_stats, 1, read);
    assert_reads(&right_stats, 1, read);
}

#[test]
fn a_one_sided_hit_that_disagrees_reads_both_sides() {
    let size = 2 * CONTENT_BLOCK;
    let left_bytes = vec![0x11; size];
    let mut right_bytes = left_bytes.clone();
    right_bytes[CONTENT_BLOCK + 5] = 0x22;
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", left_bytes.clone()).with_root("left");
    let right_source = source("a.bin", right_bytes).with_root("right");
    let left_stats = left_source.stats();
    let hash = *blake3::hash(&left_bytes).as_bytes();
    let cache = FakeCache::with(content_key(&left_source, &left_entry), hash);
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(
        result.status,
        Status::Different {
            first_difference: Some(CONTENT_BLOCK as u64 + 5)
        }
    );
    assert_eq!(left_stats.opens(), 1);
}

/// Builds a listed entry and a source under `root` whose file was modified `age_ms` ago.
fn aged(root: &str, bytes: &[u8], age_ms: i64) -> (SideEntry, FakeSource) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let modified = Some(i64::try_from(now.as_millis()).unwrap() - age_ms);
    let size = u64::try_from(bytes.len()).unwrap();
    let mut listed = entry("a.bin", size);
    listed.modified = modified;
    let mut source = source("a.bin", bytes.to_vec()).with_root(root);
    let stamp = Stamp {
        size,
        modified,
        file_id: None,
    };
    source.set_stamp("a.bin", stamp);
    (listed, source)
}

/// Checks a pair of equal files modified `age_ms` ago; returns the status and the put count.
fn check_aged(age_ms: i64) -> (Status, u64) {
    let bytes = vec![0x3C; CONTENT_BLOCK + 9];
    let (left_entry, left_source) = aged("left", &bytes, age_ms);
    let (right_entry, right_source) = aged("right", &bytes, age_ms);
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );
    (result.status, cache.puts())
}

#[test]
fn a_file_modified_five_seconds_before_the_check_is_same_but_not_stored() {
    assert_eq!(check_aged(5_000), (Status::Same, 0));
}

#[test]
fn a_file_modified_an_hour_before_the_check_is_stored() {
    assert_eq!(check_aged(3_600_000), (Status::Same, 2));
}

#[test]
fn a_file_modified_in_the_future_is_not_stored() {
    assert_eq!(check_aged(-60_000), (Status::Same, 0));
}

#[test]
fn a_one_sided_hit_stores_the_hash_under_the_read_side_key() {
    let bytes = vec![0x6D; CONTENT_BLOCK + 3];
    let hash = *blake3::hash(&bytes).as_bytes();
    let (left_entry, left_source) = aged("left", &bytes, 3_600_000);
    let (right_entry, right_source) = aged("right", &bytes, 3_600_000);
    let left_key = content_key(&left_source, &left_entry);
    let right_key = content_key(&right_source, &right_entry);
    let cancel = CancelToken::new();
    for (cached, read) in [(&left_key, &right_key), (&right_key, &left_key)] {
        let cache = FakeCache::with(cached.clone(), hash);
        let result = check(
            side(&left_entry, &left_source),
            side(&right_entry, &right_source),
            &options(&cache, &cancel),
            &mut |_| {},
        );
        assert_eq!(result.status, Status::Same);
        assert_eq!(cache.puts(), 1);
        assert_eq!(cache.lookup(read), Some(hash));
    }
}
