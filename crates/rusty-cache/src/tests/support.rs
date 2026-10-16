use std::time::SystemTime;

use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use crate::{CacheKey, CachedListing, ListingCache};

pub fn key(name: &str) -> CacheKey {
    CacheKey::new(Location::new(format!("other://{name}")).unwrap(), None)
}

pub fn entry_sized(name: &str, size: u64) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

pub fn entry(name: &str) -> FsEntry {
    entry_sized(name, 0)
}

pub fn listing(names: &[&str]) -> CachedListing {
    CachedListing {
        rows: names.iter().map(|name| entry(name)).collect(),
        read_at: SystemTime::UNIX_EPOCH,
        folder_changed: None,
        complete: true,
    }
}

pub fn names(listing: &CachedListing) -> Vec<String> {
    listing
        .rows
        .iter()
        .map(|row| row.name.display().into_owned())
        .collect()
}

pub fn cost(key: &CacheKey, listing: &CachedListing) -> usize {
    let probe = ListingCache::new(usize::MAX);
    probe.put(key.clone(), listing.clone());
    probe.stats().bytes
}

pub fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds)
}
