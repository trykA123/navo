//! Tests for the uncached content hash that verify-after-copy uses to read a target back from
//! the device (AD8b): same answer as the cached hash, cancel, errors and, on Windows, proof that
//! the handle really bypasses the cache.

use std::collections::VecDeque;
use std::io::{self, Read};

use super::ops::child;
use super::*;
use crate::local::hash::{Aligned, SECTOR_ALIGN, first_fallback_on, hash_aligned, volume_key};
use crate::{ProviderOps, ReadSource};

/// Writes `size` bytes of a repeating, non-uniform pattern to `name` and returns its location.
fn file_of(dir: &Path, root: &Location, name: &str, size: usize) -> (Location, Vec<u8>) {
    let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    fs::write(dir.join(name), &bytes).unwrap();
    (child(root, name), bytes)
}

#[test]
fn uncached_hash_equals_the_cached_hash_at_every_sector_edge() {
    let (dir, root) = temp();
    let ops = LocalFs::new();
    for size in [0, 1, 4095, 4096, 4097, 1024 * 1024 + 3] {
        let (at, bytes) = file_of(dir.path(), &root, &format!("f{size}"), size);
        let read = ops.content_hash_uncached(&at, &CancelToken::new()).unwrap();
        let uncached = read.hash;
        let cached = ops.content_hash(&at, &CancelToken::new()).unwrap();
        #[cfg(any(windows, target_os = "linux"))]
        assert_eq!(
            read.source,
            ReadSource::Device,
            "a local file is read past the cache"
        );
        assert_eq!(
            uncached, cached,
            "both reads must see the same {size} bytes"
        );
        assert_eq!(uncached, *blake3::hash(&bytes).as_bytes(), "size {size}");
    }
}

#[test]
fn uncached_hash_stops_when_cancelled() {
    let (dir, root) = temp();
    let (at, _) = file_of(dir.path(), &root, "a.bin", 4097);
    let cancel = CancelToken::new();
    cancel.cancel();
    let error = LocalFs::new()
        .content_hash_uncached(&at, &cancel)
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::Cancelled);
    assert_eq!(error.location, Some(at));
}

#[test]
fn uncached_hash_of_a_missing_file_is_not_found() {
    let (_dir, root) = temp();
    let at = child(&root, "missing.bin");
    let error = LocalFs::new()
        .content_hash_uncached(&at, &CancelToken::new())
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotFound);
}

/// One scripted read: hand out up to `n` bytes, or fail with a raw OS error code.
enum Step {
    Give(usize),
    Fail(i32),
}

/// A reader that answers each `read` with the next scripted step, then 0 (end of file).
struct Scripted {
    data: Vec<u8>,
    at: usize,
    steps: VecDeque<Step>,
}

impl Read for Scripted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.steps.pop_front() {
            None => Ok(0),
            Some(Step::Fail(code)) => Err(io::Error::from_raw_os_error(code)),
            Some(Step::Give(n)) => {
                let n = n.min(buf.len()).min(self.data.len() - self.at);
                buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
                self.at += n;
                Ok(n)
            }
        }
    }
}

/// Runs `hash_aligned` over `size` pattern bytes with `steps`, an 8-sector window and the
/// file size `expected`; returns the outcome and the bytes.
fn aligned(size: usize, expected: u64, steps: Vec<Step>) -> (io::Result<Aligned>, Vec<u8>) {
    let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let reader = Scripted {
        data: data.clone(),
        at: 0,
        steps: steps.into(),
    };
    let mut window = vec![0_u8; 8 * SECTOR_ALIGN];
    let outcome = hash_aligned(reader, &mut window, expected, &CancelToken::new());
    (outcome, data)
}

#[test]
fn short_aligned_reads_go_on_to_the_end_of_the_file() {
    let size = 3 * SECTOR_ALIGN + 5;
    let steps = vec![
        Step::Give(SECTOR_ALIGN),
        Step::Give(2 * SECTOR_ALIGN),
        Step::Give(5),
    ];
    let (outcome, data) = aligned(size, size as u64, steps);
    match outcome.unwrap() {
        Aligned::Hashed(hash) => assert_eq!(hash, *blake3::hash(&data).as_bytes()),
        other => panic!("an aligned short read is not the end of the file: {other:?}"),
    }
}

#[test]
fn an_unaligned_final_read_ends_the_file() {
    let size = 2 * SECTOR_ALIGN + 100;
    let steps = vec![Step::Give(2 * SECTOR_ALIGN), Step::Give(100), Step::Give(1)];
    let (outcome, data) = aligned(size, size as u64, steps);
    match outcome.unwrap() {
        Aligned::Hashed(hash) => assert_eq!(hash, *blake3::hash(&data).as_bytes()),
        other => panic!("the unaligned tail is the last read: {other:?}"),
    }
}

#[test]
fn a_stream_that_ends_before_the_file_size_is_an_error() {
    let size = 3 * SECTOR_ALIGN;
    let (outcome, _) = aligned(size, size as u64, vec![Step::Give(SECTOR_ALIGN)]);
    let error = outcome.expect_err("a hash of part of the file must not pass as the whole file");
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(
        error.to_string().contains("unbuffered read ended early"),
        "{error}"
    );
}

#[test]
fn a_refused_first_read_asks_for_the_cached_fallback() {
    for code in [87, 50] {
        let (outcome, _) = aligned(10, 10, vec![Step::Fail(code)]);
        match outcome.unwrap() {
            Aligned::RefusedAtStart(error) => assert_eq!(error.raw_os_error(), Some(code)),
            other => panic!("a volume refusing unbuffered reads falls back: {other:?}"),
        }
    }
}

#[test]
fn a_refusal_after_the_first_read_is_a_hard_error() {
    let size = 2 * SECTOR_ALIGN;
    let steps = vec![Step::Give(SECTOR_ALIGN), Step::Fail(87)];
    let (outcome, _) = aligned(size, size as u64, steps);
    let error = outcome.expect_err("a mid-file failure must not fall back silently");
    assert_eq!(error.raw_os_error(), Some(87));
}

#[test]
fn the_cache_fallback_warns_once_per_volume() {
    let volume = "uncached-test-volume-a";
    assert!(
        first_fallback_on(volume),
        "the first fallback on a volume warns"
    );
    assert!(
        !first_fallback_on(volume),
        "later files on the same volume stay quiet"
    );
    assert!(
        first_fallback_on("uncached-test-volume-b"),
        "another volume warns again"
    );
}

#[test]
fn files_on_one_volume_share_its_volume_key() {
    let (dir, _) = temp();
    let a = volume_key(&dir.path().join("a.bin"));
    let b = volume_key(&dir.path().join("sub").join("b.bin"));
    assert_eq!(a, b, "one warning covers every file on the volume");
    assert!(!a.is_empty(), "an absolute path has a volume root");
}

#[test]
fn the_default_uncached_hash_says_it_read_through_the_cache() {
    let fs = crate::fake::FakeFs::new();
    fs.add_file("/a.txt", 1);
    fs.set_hash("/a.txt", [7; 32]);
    let read = fs
        .content_hash_uncached(&fs.location("/a.txt"), &CancelToken::new())
        .unwrap();
    assert_eq!(read.hash, [7; 32]);
    assert_eq!(read.source, ReadSource::CacheFallback);
}

#[cfg(windows)]
#[test]
fn the_unbuffered_handle_refuses_an_unaligned_read() {
    use crate::local::hash::unbuffered_open;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.bin");
    fs::write(&path, vec![7_u8; 8192]).unwrap();
    let mut file = unbuffered_open(&path).unwrap();
    let mut one = [0_u8; 1];
    let error = file.read(&mut one).unwrap_err();
    assert_eq!(
        error.raw_os_error(),
        Some(87),
        "a cached handle would read one byte"
    );
}
