use super::{cost, key, listing};
use crate::ListingCache;

fn three_slot_cache() -> ListingCache {
    ListingCache::new(cost(&key("a"), &listing(&["x"])) * 3)
}

#[test]
fn oldest_entry_is_evicted_first() {
    let cache = three_slot_cache();
    ["a", "b", "c", "d"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    assert!(cache.get(&key("a")).is_none());
    assert!(cache.get(&key("d")).is_some());
    assert_eq!(cache.stats().evictions, 1);
}

#[test]
fn get_refreshes_recency() {
    let cache = three_slot_cache();
    ["a", "b", "c"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    let _ = cache.get(&key("a"));
    cache.put(key("d"), listing(&["x"]));
    assert!(cache.get(&key("a")).is_some());
    assert!(cache.get(&key("b")).is_none());
}

#[test]
fn entry_bigger_than_the_budget_is_not_stored() {
    let cache = ListingCache::new(cost(&key("a"), &listing(&["x"])) - 1);
    cache.put(key("a"), listing(&["x"]));
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn oversized_put_drops_the_old_listing() {
    let budget = cost(&key("a"), &listing(&["x"]));
    let cache = ListingCache::new(budget);
    cache.put(key("a"), listing(&["x"]));
    cache.put(key("a"), listing(&["x", "y"]));
    assert!(cache.get(&key("a")).is_none());
}

#[test]
fn pinned_entries_are_never_evicted() {
    let cache = three_slot_cache();
    cache.pin(&key("a"));
    ["a", "b", "c", "d", "e"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    assert!(cache.get(&key("a")).is_some());
}

#[test]
fn pin_before_put_applies_to_the_later_listing() {
    let cache = three_slot_cache();
    cache.pin(&key("a"));
    cache.put(key("a"), listing(&["x"]));
    ["b", "c", "d"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    assert!(cache.get(&key("a")).is_some());
}

#[test]
fn pinned_entries_over_budget_block_other_puts() {
    let one = cost(&key("a"), &listing(&["x"]));
    let cache = ListingCache::new(one * 2);
    ["a", "b"].iter().for_each(|name| cache.pin(&key(name)));
    cache.put(key("a"), listing(&["x"]));
    cache.put(key("b"), listing(&["x", "y"]));
    cache.put(key("c"), listing(&["x"]));
    assert!(cache.get(&key("c")).is_none());
    assert!(cache.stats().bytes > cache.stats().budget);
}

#[test]
fn unpin_makes_an_entry_evictable_again() {
    let cache = three_slot_cache();
    cache.pin(&key("a"));
    ["a", "b", "c", "d"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    cache.unpin(&key("a"));
    cache.put(key("e"), listing(&["x"]));
    assert!(cache.get(&key("a")).is_none());
}

#[test]
fn pins_are_counted() {
    let cache = three_slot_cache();
    cache.pin(&key("a"));
    cache.pin(&key("a"));
    cache.put(key("a"), listing(&["x"]));
    cache.unpin(&key("a"));
    ["b", "c", "d", "e"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    assert!(cache.get(&key("a")).is_some());
}

#[test]
fn unpin_over_budget_evicts_down_to_the_budget() {
    let one = cost(&key("a"), &listing(&["x"]));
    let cache = ListingCache::new(one * 2);
    ["a", "b", "c"].iter().for_each(|name| {
        cache.pin(&key(name));
        cache.put(key(name), listing(&["x"]));
    });
    cache.unpin(&key("a"));
    assert!(cache.get(&key("a")).is_none());
    assert_eq!(cache.stats().bytes, one * 2);
}

#[test]
fn unpin_of_an_unknown_key_is_ignored() {
    let cache = three_slot_cache();
    cache.unpin(&key("a"));
    assert_eq!(cache.pinned_bytes(), 0);
}

#[test]
fn growing_patch_evicts_older_entries() {
    let cache = three_slot_cache();
    ["a", "b", "c"]
        .iter()
        .for_each(|name| cache.put(key(name), listing(&["x"])));
    cache.patch(&key("c"), [crate::Change::Added(super::entry("y"))]);
    assert!(cache.get(&key("a")).is_none());
    assert!(cache.get(&key("c")).is_some());
}

#[test]
fn patch_past_the_budget_drops_the_entry() {
    let cache = ListingCache::new(cost(&key("a"), &listing(&["x"])));
    cache.put(key("a"), listing(&["x"]));
    cache.patch(&key("a"), [crate::Change::Added(super::entry("y"))]);
    assert!(cache.get(&key("a")).is_none());
    assert_eq!(cache.stats().bytes, 0);
}
