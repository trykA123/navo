use rusty_core::ProviderPath;

use super::support::entry_sized;
use super::{at, entry, key, listing, names};
use crate::{Change, ListingCache};

fn patched(start: &[&str], changes: Vec<Change>) -> crate::CachedListing {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(start));
    cache.patch(&key("a"), at(1), changes);
    cache.get(&key("a")).unwrap().as_ref().clone()
}

fn path(name: &str) -> ProviderPath {
    ProviderPath::from_text(name)
}

#[test]
fn added_entry_is_appended() {
    let out = patched(&["b", "a"], vec![Change::Added(entry("c"))]);
    assert_eq!(names(&out), ["b", "a", "c"]);
}

#[test]
fn added_existing_name_replaces_in_place() {
    let out = patched(&["a", "b"], vec![Change::Added(entry_sized("a", 9))]);
    assert_eq!(names(&out), ["a", "b"]);
    assert_eq!(out.rows[0].size, 9);
}

#[test]
fn removed_entry_is_dropped() {
    let out = patched(&["a", "b", "c"], vec![Change::Removed(path("b"))]);
    assert_eq!(names(&out), ["a", "c"]);
}

#[test]
fn removing_a_missing_name_changes_nothing() {
    let out = patched(&["a"], vec![Change::Removed(path("z"))]);
    assert_eq!(names(&out), ["a"]);
}

#[test]
fn renamed_entry_keeps_its_position() {
    let rename = Change::Renamed {
        from: path("b"),
        to: entry("z"),
    };
    assert_eq!(
        names(&patched(&["a", "b", "c"], vec![rename])),
        ["a", "z", "c"]
    );
}

#[test]
fn rename_over_an_existing_name_leaves_one_row() {
    let rename = Change::Renamed {
        from: path("a"),
        to: entry("c"),
    };
    assert_eq!(names(&patched(&["a", "b", "c"], vec![rename])), ["c", "b"]);
}

#[test]
fn rename_of_an_unknown_name_appends_the_target() {
    let rename = Change::Renamed {
        from: path("q"),
        to: entry("z"),
    };
    assert_eq!(names(&patched(&["a"], vec![rename])), ["a", "z"]);
}

#[test]
fn rename_to_the_same_name_updates_the_row() {
    let rename = Change::Renamed {
        from: path("a"),
        to: entry_sized("a", 4),
    };
    let out = patched(&["a", "b"], vec![rename]);
    assert_eq!(names(&out), ["a", "b"]);
    assert_eq!(out.rows[0].size, 4);
}

#[test]
fn modified_entry_replaces_the_row() {
    let out = patched(&["a"], vec![Change::Modified(entry_sized("a", 3))]);
    assert_eq!(out.rows[0].size, 3);
}

#[test]
fn patch_of_a_missing_key_is_ignored() {
    let cache = ListingCache::new(1 << 20);
    cache.patch(&key("a"), at(1), [Change::Added(entry("x"))]);
    assert!(cache.get(&key("a")).is_none());
}

#[test]
fn patch_keeps_the_listing_metadata() {
    let mut start = listing(&["a"]);
    start.complete = false;
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), start.clone());
    cache.patch(&key("a"), at(1), [Change::Added(entry("b"))]);
    let out = cache.get(&key("a")).unwrap();
    assert_eq!((out.read_at, out.complete), (at(1), false));
}
