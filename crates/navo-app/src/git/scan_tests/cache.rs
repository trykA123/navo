use super::*;
use crate::git::scan::{ScanError, listing};

fn value() -> Cached {
    listing::project(Err(ScanError::NoGit), &[], Path::new(""))
}

#[test]
fn lru_evicts_oldest_but_preserves_recently_read_status() {
    let mut cache = Cache::default();
    for index in 0..255 {
        cache.insert(PathBuf::from(format!("repo-{index}")), value());
    }
    assert_eq!(cache.entries.len(), 255);
    cache.insert("repo-255".into(), value());
    assert_eq!(cache.entries.len(), CACHE_CAP);
    assert!(cache.get(Path::new("repo-0")).is_some());
    cache.insert("repo-256".into(), value());
    assert_eq!(cache.entries.len(), CACHE_CAP);
    assert!(cache.get(Path::new("repo-0")).is_some());
    assert!(cache.get(Path::new("repo-1")).is_none());
    assert!(cache.bytes <= CACHE_BYTES);
}

#[test]
fn byte_budget_evicts_large_statuses_and_oversized_directory_maps_are_trimmed() {
    let mut cache = Cache::default();
    for index in 0..6 {
        let mut status = value();
        status.root.branch = Some("a".repeat(CACHE_BYTES / 3));
        cache.insert(format!("repo-{index}").into(), status);
    }
    assert!(cache.entries.len() < 6);
    assert!(cache.bytes <= CACHE_BYTES);
    let mut large = value();
    for index in 0..20 {
        large.dirs.insert(
            format!("{index}{}", "x".repeat(CACHE_BYTES / 10)),
            (BadgeKind::Clean, None),
        );
    }
    cache.insert("large".into(), large);
    assert!(cache.bytes <= CACHE_BYTES);
    assert!(cache.get(Path::new("large")).is_some());
}
