//! Tests that a whole compare run flushes the hash cache and reuses stored hashes.

use std::fs::{self, File};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime};

use navo_compare::model::{ContentKey, HashCache};
use navo_compare::session::Totals;
use navo_core::{Location, ProviderPath};
use navo_ui::browser::{CompareHooks, CompareSink, CompareUpdate};
use navo_ui::compare_diff::CompareOptions;

use super::file::FileHashes;
use crate::compare::handle::{CompareEvent, CompareHandle, CompareStart, CompareTarget, std_local};
use crate::compare::pane::PaneCompare;

const WAIT: Duration = Duration::from_secs(20);

/// Counts `get` hits and `flush` calls, and forwards everything to `inner`.
struct Counting {
    inner: Arc<dyn HashCache>,
    hits: AtomicUsize,
    flushes: AtomicUsize,
    puts: AtomicUsize,
}

impl Counting {
    fn new(inner: Arc<dyn HashCache>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            hits: AtomicUsize::new(0),
            flushes: AtomicUsize::new(0),
            puts: AtomicUsize::new(0),
        })
    }
}

impl HashCache for Counting {
    fn get(&self, key: &ContentKey) -> Option<[u8; 32]> {
        let hit = self.inner.get(key);
        if hit.is_some() {
            self.hits.fetch_add(1, Ordering::SeqCst);
        }
        hit
    }

    fn put(&self, key: &ContentKey, hash: [u8; 32]) {
        self.puts.fetch_add(1, Ordering::SeqCst);
        self.inner.put(key, hash);
    }

    fn flush(&self) {
        self.flushes.fetch_add(1, Ordering::SeqCst);
        self.inner.flush();
    }
}

fn loc(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap()
}

/// Compares two local folders with `cache` and returns the last totals.
pub(super) fn run_once(left: &Path, right: &Path, cache: Arc<dyn HashCache>) -> Totals {
    run_once_within(left, right, cache, WAIT)
}

/// Like `run_once`, but waits up to `wait` for the compare to finish.
pub(super) fn run_once_within(
    left: &Path,
    right: &Path,
    cache: Arc<dyn HashCache>,
    wait: Duration,
) -> Totals {
    let (sender, receiver) = mpsc::channel();
    let start = CompareStart::new(
        (
            CompareTarget::Local(loc(left)),
            CompareTarget::Local(loc(right)),
        ),
        &CompareOptions::default(),
        cache,
    );
    let _handle = CompareHandle::start(start, sender);
    let deadline = Instant::now() + wait;
    let mut totals = Totals::default();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(left).expect("compare finishes") {
            CompareEvent::Totals { totals: last, .. } => totals = last,
            CompareEvent::Finished { .. } => return totals,
            CompareEvent::Failed(error) => panic!("compare failed: {error:?}"),
            _ => {}
        }
    }
}

/// Writes each file and sets its modified time to one hour ago.
fn old_tree(dir: &Path, files: &[(&str, &[u8])]) {
    let old = SystemTime::now() - Duration::from_secs(3600);
    for (name, bytes) in files {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
}

#[test]
fn a_finished_compare_flushes_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let (left, right) = (dir.path().join("l"), dir.path().join("r"));
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    old_tree(&left, &[("a", b"same")]);
    old_tree(&right, &[("a", b"same")]);
    let cache = Counting::new(Arc::new(FileHashes::memory()));
    run_once(&left, &right, cache.clone());
    assert!(cache.flushes.load(Ordering::SeqCst) >= 1);
}

#[test]
fn stored_hashes_answer_a_second_run() {
    let dir = tempfile::tempdir().unwrap();
    let (left, right) = (dir.path().join("l"), dir.path().join("r"));
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let files: [(&str, &[u8]); 3] = [("a", b"one"), ("b", b"two"), ("c", b"three")];
    old_tree(&left, &files);
    old_tree(&right, &files);
    let path = dir.path().join("cache").join("hashes.v1");
    let first = run_once(&left, &right, Arc::new(FileHashes::open(&path)));
    assert_eq!(first.same, 3);
    let second = Counting::new(Arc::new(FileHashes::open(&path)));
    let totals = run_once(&left, &right, second.clone());
    assert_eq!(totals.same, 3);
    assert_eq!(second.hits.load(Ordering::SeqCst), 6);
}

/// Starts a pane compare of two folders and waits for `Finished` on the sink.
pub(super) fn pane_run(left: &Path, right: &Path, cache: Arc<dyn HashCache>, fresh_hashes: bool) {
    let (sender, receiver) = mpsc::channel();
    let sink: CompareSink = Arc::new(move |update| {
        if matches!(update, CompareUpdate::Finished { .. }) {
            let _ = sender.send(());
        }
    });
    let options = CompareOptions {
        fresh_hashes,
        ..CompareOptions::default()
    };
    let hooks = PaneCompare::new(cache, std_local());
    let _control = hooks.start((&loc(left), &loc(right)), &options, sink);
    receiver.recv_timeout(WAIT).expect("compare finishes");
}

#[test]
fn a_recheck_start_misses_every_stored_hash() {
    let dir = tempfile::tempdir().unwrap();
    let (left, right) = (dir.path().join("l"), dir.path().join("r"));
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let files: [(&str, &[u8]); 2] = [("a", b"one"), ("b", b"two")];
    old_tree(&left, &files);
    old_tree(&right, &files);
    let store: Arc<dyn HashCache> = Arc::new(FileHashes::memory());
    run_once(&left, &right, store.clone());
    let fresh = Counting::new(store.clone());
    pane_run(&left, &right, fresh.clone(), true);
    assert_eq!(fresh.hits.load(Ordering::SeqCst), 0);
    assert!(fresh.puts.load(Ordering::SeqCst) > 0);
    let reuse = Counting::new(store);
    pane_run(&left, &right, reuse.clone(), false);
    assert!(reuse.hits.load(Ordering::SeqCst) > 0);
}
