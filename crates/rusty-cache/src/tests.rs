mod support;

mod budget;
mod patch;
mod props;
mod race;

use support::{at, cost, entry, key, listing, names};

use crate::{DEFAULT_BUDGET, ListingCache};

#[test]
fn default_budget_is_64_mib() {
    assert_eq!(ListingCache::default().stats().budget, 64 * 1024 * 1024);
    assert_eq!(DEFAULT_BUDGET, 64 * 1024 * 1024);
}

#[test]
fn get_after_put_returns_the_listing() {
    let cache = ListingCache::new(1 << 20);
    let stored = listing(&["a", "b"]);
    cache.put(key("a"), stored.clone());
    assert_eq!(cache.get(&key("a")).as_deref(), Some(&stored));
}

#[test]
fn get_of_a_missing_key_is_none_and_counts_a_miss() {
    let cache = ListingCache::new(1 << 20);
    assert!(cache.get(&key("a")).is_none());
    assert_eq!(cache.stats().misses, 1);
}

#[test]
fn hits_are_counted() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["x"]));
    let _ = cache.get(&key("a"));
    let _ = cache.get(&key("a"));
    assert_eq!(cache.stats().hits, 2);
}

#[test]
fn volume_serial_is_part_of_the_key() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["x"]));
    let other = crate::CacheKey::new(key("a").location, Some(7));
    assert!(cache.get(&other).is_none());
}

#[test]
fn put_replaces_and_keeps_bytes_exact() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["x", "y", "z"]));
    cache.put(key("a"), listing(&["x"]));
    assert_eq!(cache.stats().bytes, cost(&key("a"), &listing(&["x"])));
    assert_eq!(cache.stats().entries, 1);
}

#[test]
fn invalidate_removes_the_entry() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["x"]));
    cache.invalidate(&key("a"));
    assert!(cache.get(&key("a")).is_none());
    assert_eq!(cache.stats().bytes, 0);
}

#[test]
fn clones_share_one_cache() {
    let cache = ListingCache::new(1 << 20);
    let other = cache.clone();
    other.put(key("a"), listing(&["x"]));
    assert!(cache.get(&key("a")).is_some());
}

#[test]
fn cache_is_send_and_sync_across_threads() {
    let cache = ListingCache::new(1 << 20);
    let worker = cache.clone();
    std::thread::spawn(move || worker.put(key("a"), listing(&["x"])))
        .join()
        .unwrap();
    assert_eq!(names(&cache.get(&key("a")).unwrap()), ["x"]);
}

#[test]
fn readers_keep_their_snapshot_after_a_patch() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["x"]));
    let before = cache.get(&key("a")).unwrap();
    cache.patch(&key("a"), at(1), [crate::Change::Added(entry("y"))]);
    assert_eq!(names(&before), ["x"]);
}
