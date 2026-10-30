mod bench;
mod fake;
mod large;
mod stamps;

use fake::{FakeCache, assert_reads, entry, kind_entry, options, side, source};

use super::read::content_key;
use super::*;
use crate::model::{CancelToken, ContentClass, EntryKind, ObjectId, ReadHint, Reason, Status};

#[test]
fn equal_object_ids_read_nothing() {
    let object_id = ObjectId::Sha1([0xA1; 20]);
    let left_entry = entry("a.bin", 4);
    let right_entry = entry("a.bin", 4);
    let left_source = source("a.bin", b"data".to_vec()).with_object_id("a.bin", object_id);
    let right_source = source("a.bin", b"data".to_vec()).with_object_id("a.bin", object_id);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let mut events = Vec::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |event| events.push(event),
    );

    assert_eq!(
        result,
        ContentResult {
            status: Status::Same,
            content: ContentClass::Unknown,
        }
    );
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 0, 0);
    assert!(events.is_empty());
    assert_eq!(cache.puts(), 0);
}

#[test]
fn different_object_ids_are_different_without_reading() {
    let left_entry = entry("a.bin", 4);
    let right_entry = entry("a.bin", 4);
    let left_source =
        source("a.bin", b"data".to_vec()).with_object_id("a.bin", ObjectId::Sha1([0x11; 20]));
    let right_source =
        source("a.bin", b"data".to_vec()).with_object_id("a.bin", ObjectId::Sha1([0x22; 20]));
    let left_stats = left_source.stats();
    let cache = FakeCache::default();
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
            first_difference: None
        }
    );
    assert_reads(&left_stats, 0, 0);
}

#[test]
fn different_sizes_read_nothing() {
    let left_entry = entry("a.bin", 3);
    let right_entry = entry("a.bin", 4);
    let left_source = source("a.bin", b"abc".to_vec());
    let right_source = source("a.bin", b"abcd".to_vec());
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
        result.status,
        Status::Different {
            first_difference: None
        }
    );
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 0, 0);
}

#[test]
fn kinds_short_circuit_before_content() {
    let file = entry("a.bin", 0);
    let folder = kind_entry("a.bin", EntryKind::Folder);
    let link = kind_entry("a.bin", EntryKind::Link);
    let left_source = source("a.bin", Vec::new());
    let right_source = source("a.bin", Vec::new());
    let left_stats = left_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();

    let conflict = check(
        side(&file, &left_source),
        side(&folder, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );
    assert_eq!(conflict.status, Status::TypeConflict);

    let folders = check(
        side(&folder, &left_source),
        side(&folder, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );
    assert_eq!(folders.status, Status::Same);

    let links = check(
        side(&link, &left_source),
        side(&link, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );
    assert_eq!(links.status, Status::Unavailable(Reason::Link));
    assert_reads(&left_stats, 0, 0);
}

#[test]
fn an_equal_pair_hashes_both_sides_and_fills_the_cache() {
    let size = 2 * CONTENT_BLOCK + 17;
    let bytes = vec![0xAB; size];
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", bytes.clone()).with_root("left");
    let right_source = source("a.bin", bytes.clone()).with_root("right");
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let mut events = Vec::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |event| events.push(event),
    );

    let expected_hash = *blake3::hash(&bytes).as_bytes();
    assert_eq!(result.status, Status::Same);
    assert_eq!(result.content, ContentClass::Text);
    assert_reads(&left_stats, 1, u64::try_from(size).unwrap());
    assert_reads(&right_stats, 1, u64::try_from(size).unwrap());
    assert_eq!(cache.puts(), 2);
    assert_eq!(
        cache.lookup(&content_key(&left_source, &left_entry)),
        Some(expected_hash)
    );
    assert_eq!(
        cache.lookup(&content_key(&right_source, &right_entry)),
        Some(expected_hash)
    );
    assert_eq!(events.len(), 3);
    assert_eq!(
        events.last(),
        Some(&ContentProgress {
            bytes_done: u64::try_from(size).unwrap(),
            bytes_total: u64::try_from(size).unwrap(),
        })
    );
}

#[test]
fn a_pair_differing_at_ten_percent_reads_about_ten_percent() {
    let size = 10 * CONTENT_BLOCK;
    let left_bytes = vec![0xAA; size];
    let mut right_bytes = left_bytes.clone();
    right_bytes[CONTENT_BLOCK - 1] = 0xAB;
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", left_bytes);
    let right_source = source("a.bin", right_bytes);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let mut events = Vec::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |event| events.push(event),
    );

    assert_eq!(
        result.status,
        Status::Different {
            first_difference: Some(u64::try_from(CONTENT_BLOCK - 1).unwrap()),
        }
    );
    assert_eq!(result.content, ContentClass::Unknown);
    let block = u64::try_from(CONTENT_BLOCK).unwrap();
    assert_reads(&left_stats, 1, block);
    assert_reads(&right_stats, 1, block);
    assert_eq!(
        events.as_slice(),
        [ContentProgress {
            bytes_done: block,
            bytes_total: u64::try_from(size).unwrap(),
        }]
    );
}

#[test]
fn a_cache_hit_reads_one_side_only() {
    let size = 2 * CONTENT_BLOCK;
    let bytes = vec![0x5A; size];
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", bytes.clone()).with_root("left");
    let right_source = source("a.bin", bytes.clone()).with_root("right");
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let hash = *blake3::hash(&bytes).as_bytes();
    let cache = FakeCache::with(content_key(&left_source, &left_entry), hash);
    let cancel = CancelToken::new();
    let mut events = Vec::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |event| events.push(event),
    );

    assert_eq!(result.status, Status::Same);
    assert_eq!(result.content, ContentClass::Text);
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 1, u64::try_from(size).unwrap());
    assert_eq!(cache.puts(), 1);
    assert_eq!(
        cache.lookup(&content_key(&right_source, &right_entry)),
        Some(hash)
    );
    assert_eq!(right_stats.hints(), vec![ReadHint::Buffered]);
    assert_eq!(events.len(), 2);
}

#[test]
fn a_nul_marks_the_pair_binary() {
    let bytes = b"a\0bc".to_vec();
    let left_entry = entry("a.bin", u64::try_from(bytes.len()).unwrap());
    let right_entry = entry("a.bin", u64::try_from(bytes.len()).unwrap());
    let left_source = source("a.bin", bytes.clone());
    let right_source = source("a.bin", bytes);
    let cache = FakeCache::default();
    let cancel = CancelToken::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |_| {},
    );

    assert_eq!(result.status, Status::Same);
    assert_eq!(result.content, ContentClass::Binary);
}

#[test]
fn cancel_stops_between_blocks() {
    let size = 2 * CONTENT_BLOCK;
    let bytes = vec![0x11; size];
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", bytes.clone());
    let right_source = source("a.bin", bytes);
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let stop = cancel.clone();
    let mut events = Vec::new();

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options(&cache, &cancel),
        &mut |event| {
            events.push(event);
            stop.cancel();
        },
    );

    assert_eq!(
        result,
        ContentResult {
            status: Status::NotChecked(Reason::Stopped),
            content: ContentClass::Unknown,
        }
    );
    assert_eq!(events.len(), 1);
    let block = u64::try_from(CONTENT_BLOCK).unwrap();
    assert_reads(&left_stats, 1, block);
    assert_reads(&right_stats, 1, block);
}

#[test]
fn a_read_error_is_unavailable() {
    let size = CONTENT_BLOCK + 10;
    let bytes = vec![0x77; size];
    let left_entry = entry("a.bin", u64::try_from(size).unwrap());
    let right_entry = entry("a.bin", u64::try_from(size).unwrap());
    let left_source = source("a.bin", bytes.clone());
    let right_source =
        source("a.bin", bytes).failing_after("a.bin", u64::try_from(CONTENT_BLOCK).unwrap());
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
            status: Status::Unavailable(Reason::ReadError),
            content: ContentClass::Unknown,
        }
    );
}

#[test]
fn known_same_reads_nothing_and_skips_the_cache() {
    let left_entry = entry("a.bin", 4);
    let right_entry = entry("a.bin", 4);
    let left_source = source("a.bin", b"data".to_vec());
    let right_source = source("a.bin", b"data".to_vec());
    let left_stats = left_source.stats();
    let right_stats = right_source.stats();
    let cache = FakeCache::default();
    let cancel = CancelToken::new();
    let options = CheckOptions {
        cache: &cache,
        cancel: &cancel,
        read_hint: ReadHint::Buffered,
        known_same: true,
    };

    let result = check(
        side(&left_entry, &left_source),
        side(&right_entry, &right_source),
        &options,
        &mut |_| {},
    );

    assert_eq!(
        result,
        ContentResult {
            status: Status::Same,
            content: ContentClass::Unknown,
        }
    );
    assert_reads(&left_stats, 0, 0);
    assert_reads(&right_stats, 0, 0);
    assert_eq!(cache.gets(), 0);
    assert_eq!(cache.puts(), 0);
}
