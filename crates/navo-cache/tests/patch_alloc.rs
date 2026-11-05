#![allow(
    clippy::print_stdout,
    reason = "the test prints its three measured numbers for the CI log"
)]
#![allow(clippy::expect_used, reason = "the test fails on a broken fixture")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use navo_cache::{CacheKey, CachedListing, Change, ListingCache};
use navo_core::{Location, ProviderPath};
use navo_fs::{Attributes, EntryKind, FsEntry};

const ROWS: usize = 100_000;
const SLACK: usize = 64 * 1024;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every operation forwards to the system allocator unchanged; the counter only records sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller upholds the GlobalAlloc contract, forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds the GlobalAlloc contract, forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATED.fetch_add(new_size, Ordering::Relaxed);
        // SAFETY: the caller upholds the GlobalAlloc contract, forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn key() -> CacheKey {
    CacheKey::new(
        Location::new("other://patch-alloc").expect("the test location is valid"),
        None,
    )
}

fn entry(index: usize) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(&format!("row-{index:06}.dat")),
        kind: EntryKind::File,
        size: index as u64,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn base_rows() -> Arc<[FsEntry]> {
    (0..ROWS).map(entry).collect()
}

fn listing(rows: Arc<[FsEntry]>) -> CachedListing {
    CachedListing {
        rows,
        read_at: at(0),
        folder_changed: None,
        complete: true,
    }
}

fn reset() {
    ALLOCATED.store(0, Ordering::SeqCst);
}

fn measured() -> usize {
    ALLOCATED.load(Ordering::SeqCst)
}

fn row_clone_bytes(rows: &[FsEntry]) -> usize {
    rows.iter()
        .map(|row| size_of::<FsEntry>() + row.name.display().len())
        .sum()
}

// The pre-plan patch path, kept here as the allocation baseline.
struct IndexedRows {
    rows: Vec<Option<FsEntry>>,
    index: HashMap<ProviderPath, usize>,
}

impl IndexedRows {
    fn new(rows: &[FsEntry]) -> Self {
        let index = rows
            .iter()
            .enumerate()
            .map(|(at, row)| (row.name.clone(), at))
            .collect();
        Self {
            rows: rows.iter().cloned().map(Some).collect(),
            index,
        }
    }

    fn apply(&mut self, change: &Change) {
        match change {
            Change::Added(entry) | Change::Modified(entry) => self.upsert(entry.clone()),
            Change::Removed(name) => self.remove(name),
            Change::Renamed { from, to } => self.rename(from, to.clone()),
        }
    }

    fn upsert(&mut self, entry: FsEntry) {
        match self.index.get(&entry.name) {
            Some(&at) => self.rows[at] = Some(entry),
            None => self.push(entry),
        }
    }

    fn push(&mut self, entry: FsEntry) {
        self.index.insert(entry.name.clone(), self.rows.len());
        self.rows.push(Some(entry));
    }

    fn remove(&mut self, name: &ProviderPath) {
        if let Some(at) = self.index.remove(name) {
            self.rows[at] = None;
        }
    }

    fn rename(&mut self, from: &ProviderPath, to: FsEntry) {
        if *from != to.name {
            self.remove(&to.name);
        }
        match self.index.remove(from) {
            Some(at) => {
                self.index.insert(to.name.clone(), at);
                self.rows[at] = Some(to);
            }
            None => self.push(to),
        }
    }

    fn into_rows(self) -> Arc<[FsEntry]> {
        self.rows.into_iter().flatten().collect()
    }
}

fn oracle_rows(rows: &[FsEntry], changes: &[Change]) -> Arc<[FsEntry]> {
    let mut oracle = IndexedRows::new(rows);
    for change in changes {
        oracle.apply(change);
    }
    oracle.into_rows()
}

#[test]
fn a_warm_modified_patch_clones_the_rows_without_rebuilding_the_index() {
    let base = base_rows();
    let clone_bytes = row_clone_bytes(&base);
    let cache = ListingCache::new(usize::MAX);
    cache.put(key(), listing(Arc::clone(&base)));
    cache.patch(&key(), at(1), [Change::Modified(entry(ROWS / 2))]);

    reset();
    cache.patch(&key(), at(2), [Change::Modified(entry(ROWS / 3))]);
    let warm = measured();

    reset();
    let patched = oracle_rows(&base, &[Change::Modified(entry(ROWS / 3))]);
    let oracle = measured();
    drop(patched);

    println!("patch alloc: warm patch {warm} B, row clone {clone_bytes} B, oracle {oracle} B");
    assert!(
        warm <= clone_bytes + SLACK,
        "warm patch allocated {warm} B, over the row clone {clone_bytes} B plus {SLACK} B"
    );
    assert!(
        warm * 10 <= oracle * 6,
        "warm patch allocated {warm} B, over 0.6 of the oracle {oracle} B"
    );
    let after = cache.get(&key()).expect("the patched listing is cached");
    assert_eq!(after.rows[ROWS / 3].size, (ROWS / 3) as u64);
}
