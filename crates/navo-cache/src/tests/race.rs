use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::support::entry_sized;
use super::{at, entry, key, listing, names};
use crate::store::SizedListing;
use crate::{CachedListing, Change, ListingCache};

fn listing_at(names: &[&str], seconds: u64) -> CachedListing {
    CachedListing {
        read_at: at(seconds),
        ..listing(names)
    }
}

fn big_listing(rows: usize) -> CachedListing {
    CachedListing {
        rows: (0..rows).map(|i| entry(&format!("row-{i}"))).collect(),
        ..listing(&[])
    }
}

#[test]
fn gets_are_not_blocked_during_a_large_patch() {
    let cache = ListingCache::new(usize::MAX);
    cache.put(key("big"), big_listing(100_000));
    cache.put(key("small"), listing(&["x"]));
    let changes: Vec<Change> = (0..1_000)
        .map(|i| Change::Modified(entry_sized(&format!("row-{}", i * 97), 1)))
        .collect();
    let done = Arc::new(AtomicBool::new(false));
    let patcher = {
        let (cache, done) = (cache.clone(), Arc::clone(&done));
        std::thread::spawn(move || {
            let started = Instant::now();
            cache.patch(&key("big"), at(1), changes);
            done.store(true, Ordering::Release);
            started.elapsed()
        })
    };
    let mut slowest = Duration::ZERO;
    while !done.load(Ordering::Acquire) {
        let started = Instant::now();
        assert!(cache.get(&key("small")).is_some());
        slowest = slowest.max(started.elapsed());
    }
    let patch_time = patcher.join().unwrap();
    assert!(
        slowest * 4 < patch_time,
        "get {slowest:?}, patch {patch_time:?}"
    );
    assert_eq!(cache.get(&key("big")).unwrap().rows[97].size, 1);
}

#[test]
fn stale_patch_does_not_overwrite_a_racing_put() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing_at(&["old"], 1));
    let seen = cache.lock().peek(&key("a")).unwrap();
    cache.put(key("a"), listing_at(&["fresh"], 2));
    let patched = seen.patched(&[Change::Added(entry("added"))], at(1));
    let swap = cache
        .lock()
        .swap(&key("a"), &seen, SizedListing::new(&key("a"), patched));
    assert_eq!(swap, crate::store::Swap::Stale);
    assert_eq!(names(&cache.get(&key("a")).unwrap()), ["fresh"]);
}

#[test]
fn patch_older_than_the_cached_listing_is_ignored() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing_at(&["fresh"], 5));
    cache.patch(&key("a"), at(4), [Change::Added(entry("late"))]);
    assert_eq!(names(&cache.get(&key("a")).unwrap()), ["fresh"]);
}

#[test]
fn revalidation_started_before_a_patch_does_not_overwrite_it() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing_at(&["x"], 1));
    cache.patch(&key("a"), at(3), [Change::Added(entry("y"))]);
    cache.put(key("a"), listing_at(&["x"], 2));
    assert_eq!(names(&cache.get(&key("a")).unwrap()), ["x", "y"]);
}

#[test]
fn newer_revalidation_replaces_a_patched_listing() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing_at(&["x"], 1));
    cache.patch(&key("a"), at(3), [Change::Added(entry("y"))]);
    cache.put(key("a"), listing_at(&["z"], 4));
    assert_eq!(names(&cache.get(&key("a")).unwrap()), ["z"]);
}

#[test]
fn swap_on_a_missing_key_reports_missing() {
    let cache = ListingCache::new(1 << 20);
    let seen = Arc::new(listing(&["x"]));
    let entry = SizedListing::new(&key("a"), listing(&["x"]));
    assert_eq!(
        cache.lock().swap(&key("a"), &seen, entry),
        crate::store::Swap::Missing
    );
}
