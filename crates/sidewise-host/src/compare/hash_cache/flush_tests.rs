//! Tests that the session thread flushes the hash cache before it reports `Finished`.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use navo_compare::model::{ContentKey, HashCache};

use super::reuse_tests::run_once;

/// A cache whose flush is slow, like a large append to disk; counts finished flushes.
#[derive(Default)]
struct SlowFlush {
    flushed: AtomicUsize,
}

impl HashCache for SlowFlush {
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &ContentKey, _hash: [u8; 32]) {}

    fn flush(&self) {
        thread::sleep(Duration::from_millis(300));
        self.flushed.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn the_cache_is_flushed_before_finished_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (left, right) = (dir.path().join("l"), dir.path().join("r"));
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("a"), b"same").unwrap();
    fs::write(right.join("a"), b"same").unwrap();
    let cache = Arc::new(SlowFlush::default());
    run_once(&left, &right, cache.clone());
    assert!(
        cache.flushed.load(Ordering::SeqCst) >= 1,
        "Finished came before the flush"
    );
}
