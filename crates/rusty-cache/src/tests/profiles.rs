use std::sync::Arc;

use rusty_core::Location;
use rusty_fs::{FolderProfile, FolderSize, ProfileState, Ranking, Tally};

use crate::ProfileCache;

fn at(path: &str) -> Location {
    Location::new(path).unwrap()
}

fn profile(bytes: u64, state: ProfileState) -> FolderProfile {
    FolderProfile {
        totals: FolderSize {
            bytes,
            ..FolderSize::default()
        },
        newest_ms: None,
        kinds: Vec::new(),
        other: Tally::default(),
        largest: Vec::new(),
        ranking: Ranking::default(),
        counted_ms: None,
        state,
    }
}

#[test]
fn an_unchanged_folder_hits() {
    let cache = ProfileCache::default();
    cache.put(at("/d"), 10, profile(5, ProfileState::Done));
    let hit = cache.get(&at("/d"), 10);
    assert_eq!(hit.map(|p| p.totals.bytes), Some(5));
}

#[test]
fn a_changed_folder_misses_and_drops_the_stale_profile() {
    let cache = ProfileCache::default();
    cache.put(at("/d"), 10, profile(5, ProfileState::Done));
    assert!(cache.get(&at("/d"), 11).is_none());
    assert!(cache.get(&at("/d"), 10).is_none());
    assert_eq!(cache.len(), 0);
}

#[test]
fn only_finished_profiles_are_kept() {
    let cache = ProfileCache::default();
    cache.put(at("/a"), 1, profile(1, ProfileState::Running));
    cache.put(at("/b"), 1, profile(1, ProfileState::Stopped));
    cache.put(
        at("/c"),
        1,
        profile(
            1,
            ProfileState::Failed(rusty_core::FsErrorKind::AccessDenied),
        ),
    );
    assert_eq!(cache.len(), 0);
}

#[test]
fn the_least_recently_used_folder_is_evicted() {
    let cache = ProfileCache::new(2);
    cache.put(at("/a"), 1, profile(1, ProfileState::Done));
    cache.put(at("/b"), 1, profile(2, ProfileState::Done));
    assert!(cache.get(&at("/a"), 1).is_some());
    cache.put(at("/c"), 1, profile(3, ProfileState::Done));
    assert!(cache.get(&at("/b"), 1).is_none());
    assert!(cache.get(&at("/a"), 1).is_some());
    assert!(cache.get(&at("/c"), 1).is_some());
}

#[test]
fn clones_share_one_cache() {
    let cache = ProfileCache::default();
    let other = cache.clone();
    other.put(at("/d"), 3, profile(9, ProfileState::Done));
    let hit: Option<Arc<FolderProfile>> = cache.get(&at("/d"), 3);
    assert!(hit.is_some());
    assert_eq!(ProfileCache::default().capacity(), 256);
}

#[test]
fn invalidating_a_folder_drops_it_its_parents_and_everything_inside() {
    let cache = ProfileCache::default();
    for path in ["/w", "/w/d", "/w/d/inner", "/w/other", "/x"] {
        cache.put(at(path), 1, profile(1, ProfileState::Done));
    }
    cache.invalidate(&at("/w/d"));
    let kept: Vec<bool> = ["/w", "/w/d", "/w/d/inner", "/w/other", "/x"]
        .iter()
        .map(|path| cache.get(&at(path), 1).is_some())
        .collect();
    assert_eq!(kept, [false, false, false, true, true]);
}
