use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use super::*;

const PAGE: u64 = PAGE_SIZE as u64;

fn byte_at(offset: u64) -> u8 {
    u8::try_from(offset % 251).unwrap()
}

fn pattern(len: u64) -> Vec<u8> {
    (0..len).map(byte_at).collect()
}

fn file_with(len: u64) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.bin");
    fs::write(&path, pattern(len)).unwrap();
    (dir, path)
}

fn read(file: &PagedFile, offset: u64, count: usize) -> Vec<u8> {
    let mut buf = vec![0; count];
    let got = file.read_at(offset, &mut buf).unwrap();
    buf.truncate(got);
    buf
}

fn expected(offset: u64, count: u64) -> Vec<u8> {
    (offset..offset + count).map(byte_at).collect()
}

#[test]
fn opening_a_missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(PagedFile::open(&dir.path().join("nope.bin")).is_err());
}

#[test]
fn opening_a_folder_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(PagedFile::open(dir.path()).is_err());
}

#[test]
fn len_is_the_file_length() {
    let (_dir, path) = file_with(3 * PAGE + 17);
    let file = PagedFile::open(&path).unwrap();
    assert_eq!(file.len(), 3 * PAGE + 17);
}

#[test]
fn an_empty_file_has_zero_length_and_reads_nothing() {
    let (_dir, path) = file_with(0);
    let file = PagedFile::open(&path).unwrap();
    assert_eq!(file.len(), 0);
    assert!(read(&file, 0, 10).is_empty());
}

#[test]
fn read_at_returns_the_bytes_at_the_offset() {
    let (_dir, path) = file_with(2 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    assert_eq!(read(&file, 1000, 300), expected(1000, 300));
}

#[test]
fn a_read_across_several_pages_is_filled_completely() {
    let (_dir, path) = file_with(4 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let count = 2 * PAGE_SIZE + 100;
    assert_eq!(
        read(&file, PAGE - 50, count),
        expected(PAGE - 50, 2 * PAGE + 100)
    );
}

#[test]
fn a_read_at_the_end_is_short() {
    let (_dir, path) = file_with(PAGE + 10);
    let file = PagedFile::open(&path).unwrap();
    assert_eq!(read(&file, PAGE + 4, 100), expected(PAGE + 4, 6));
}

#[test]
fn a_read_at_or_past_the_end_returns_zero() {
    let (_dir, path) = file_with(PAGE + 10);
    let file = PagedFile::open(&path).unwrap();
    assert!(read(&file, PAGE + 10, 8).is_empty());
    assert!(read(&file, 10 * PAGE, 8).is_empty());
}

#[test]
fn reading_the_same_range_twice_costs_one_disk_read() {
    let (_dir, path) = file_with(2 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let first = read(&file, 100, 200);
    let second = read(&file, 100, 200);
    assert_eq!(first, second);
    assert_eq!(file.disk_reads(), 1);
}

#[test]
fn pages_are_aligned_to_page_size() {
    let (_dir, path) = file_with(3 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, PAGE - 1, 1);
    let _ = read(&file, 0, 1);
    let _ = read(&file, PAGE, 1);
    let _ = read(&file, 2 * PAGE - 1, 1);
    assert_eq!(file.disk_reads(), 2);
}

#[test]
fn a_read_spanning_two_pages_costs_two_disk_reads() {
    let (_dir, path) = file_with(3 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, PAGE - 10, 20);
    assert_eq!(file.disk_reads(), 2);
    assert_eq!(file.cached_pages(), 2);
}

#[test]
fn the_least_recently_used_page_is_evicted() {
    let pages = u64::try_from(CACHE_PAGES).unwrap() + 2;
    let (_dir, path) = file_with(pages * PAGE);
    let file = PagedFile::open(&path).unwrap();
    for page in 0..=u64::try_from(CACHE_PAGES).unwrap() {
        let _ = read(&file, page * PAGE, 1);
        assert!(file.cached_pages() <= CACHE_PAGES);
    }
    let loaded = file.disk_reads();
    assert_eq!(loaded, u64::try_from(CACHE_PAGES).unwrap() + 1);
    assert_eq!(file.cached_pages(), CACHE_PAGES);
    let _ = read(&file, 0, 1);
    assert_eq!(file.disk_reads(), loaded + 1);
    assert_eq!(file.cached_pages(), CACHE_PAGES);
}

#[test]
fn a_recently_used_page_survives_eviction() {
    let (_dir, path) = file_with(40 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 1);
    for page in 1..u64::try_from(CACHE_PAGES).unwrap() {
        let _ = read(&file, page * PAGE, 1);
    }
    let _ = read(&file, 0, 1);
    let _ = read(&file, 35 * PAGE, 1);
    let loaded = file.disk_reads();
    let _ = read(&file, 0, 1);
    assert_eq!(file.disk_reads(), loaded);
    let _ = read(&file, PAGE, 1);
    assert_eq!(file.disk_reads(), loaded + 1);
}

#[test]
fn the_open_file_can_be_renamed() {
    let (dir, path) = file_with(PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 10);
    fs::rename(&path, dir.path().join("moved.bin")).unwrap();
    assert_eq!(file.cached_pages(), 1);
}

#[test]
fn the_open_file_can_be_removed() {
    let (_dir, path) = file_with(PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 10);
    fs::remove_file(&path).unwrap();
    assert!(!path.exists());
    assert_eq!(file.cached_pages(), 1);
}

fn append(path: &Path, bytes: &[u8]) {
    let mut handle = OpenOptions::new().append(true).open(path).unwrap();
    handle.write_all(bytes).unwrap();
    let later = SystemTime::now() + Duration::from_secs(60);
    handle.set_modified(later).unwrap();
}

#[test]
fn a_page_miss_after_the_file_grew_reports_a_change() {
    let (_dir, path) = file_with(2 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 10);
    assert!(!file.take_changed());
    append(&path, b"appended tail");
    let _ = read(&file, PAGE, 10);
    assert!(file.take_changed());
    assert_eq!(file.len(), 2 * PAGE + 13);
    assert_eq!(read(&file, 2 * PAGE, 64), b"appended tail");
    assert!(!file.take_changed());
}

#[test]
fn a_change_drops_the_cache() {
    let (_dir, path) = file_with(3 * PAGE);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 10);
    append(&path, b"more");
    let _ = read(&file, 2 * PAGE, 10);
    assert!(file.take_changed());
    let loaded = file.disk_reads();
    let _ = read(&file, 0, 10);
    assert_eq!(file.disk_reads(), loaded + 1);
}

#[test]
fn reading_past_the_end_notices_a_grown_file_whose_tail_is_cached() {
    let (_dir, path) = file_with(100);
    let file = PagedFile::open(&path).unwrap();
    assert_eq!(read(&file, 0, 200), expected(0, 100));
    append(&path, b"new lines");
    assert_eq!(read(&file, 100, 64), b"");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(read(&file, 100, 64), b"new lines");
    assert!(file.take_changed());
    assert_eq!(file.len(), 109);
}

#[test]
fn length_and_change_flag_never_wait_for_a_page_read() {
    let (_dir, path) = file_with(PAGE);
    let file = PagedFile::open(&path).unwrap();
    let held = file.cache();
    assert_eq!(file.len(), PAGE);
    assert!(!file.take_changed());
    drop(held);
}

#[test]
fn an_older_stat_never_undoes_a_newer_one() {
    let (_dir, path) = file_with(PAGE);
    let file = PagedFile::open(&path).unwrap();
    let older = file.stat();
    append(&path, b"later bytes");
    let newer = file.stat();
    assert!(file.apply(&mut file.cache(), newer));
    assert_eq!(file.len(), PAGE + 11);
    assert!(!file.apply(&mut file.cache(), older));
    assert_eq!(file.len(), PAGE + 11);
}

fn replace_by_rename(dir: &TempDir, path: &Path, bytes: &[u8]) {
    let fresh = dir.path().join("fresh.tmp");
    fs::write(&fresh, bytes).unwrap();
    fs::rename(&fresh, path).unwrap();
}

#[test]
fn a_file_replaced_by_rename_is_reopened_and_reported() {
    let (dir, path) = file_with(PAGE);
    let file = PagedFile::open(&path).unwrap();
    assert_eq!(read(&file, 0, 4), expected(0, 4));
    replace_by_rename(&dir, &path, b"a fresh log line\n");
    std::thread::sleep(Duration::from_millis(300));
    let _ = read(&file, file.len(), 1);
    assert!(file.take_changed());
    assert!(file.take_rewritten());
    assert_eq!(file.len(), 17);
    assert_eq!(read(&file, 0, 64), b"a fresh log line\n");
    assert!(!file.take_rewritten());
}

#[test]
fn growth_in_place_is_not_a_rewrite() {
    let (_dir, path) = file_with(100);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 100);
    append(&path, b"new lines");
    std::thread::sleep(Duration::from_millis(300));
    let _ = read(&file, 100, 64);
    assert!(file.take_changed());
    assert!(!file.take_rewritten());
}

#[test]
fn a_file_rewritten_in_place_and_grown_is_reported() {
    let (_dir, path) = file_with(100);
    let file = PagedFile::open(&path).unwrap();
    let _ = read(&file, 0, 100);
    let mut rewritten = vec![b'x'; 100];
    rewritten.extend_from_slice(b"new lines");
    fs::write(&path, &rewritten).unwrap();
    let later = SystemTime::now() + Duration::from_secs(60);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = read(&file, 100, 64);
    assert!(file.take_changed());
    assert!(file.take_rewritten());
    assert_eq!(read(&file, 0, 4), b"xxxx");
}
