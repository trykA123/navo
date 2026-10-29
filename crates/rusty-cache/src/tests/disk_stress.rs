use std::fmt::Write as _;
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_store::CacheDb;

use super::support::listing;
use crate::disk::codec;
use crate::{CacheKeys, CachedListing, DISK_BUDGET, DiskListings, ListingCache, Warm};

fn stress_listing() -> CachedListing {
    let mut state = 7_u64;
    let names: Vec<String> = (0..64)
        .map(|_| {
            let mut name = String::new();
            for _ in 0..4 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                write!(name, "{state:016x}").unwrap();
            }
            name
        })
        .collect();
    listing(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

#[test]
#[ignore]
fn stress_50_000_disk_listings_stay_within_budget_and_match_database_usage() {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(60);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let cache = ListingCache::default();
    let keys = CacheKeys::default();
    let disk = DiskListings::open(
        &path,
        Warm {
            cache: cache.clone(),
            keys: keys.clone(),
            folders: Vec::new(),
            deadline,
        },
    );
    let (db, _) = CacheDb::open(&path).unwrap();
    let listing = stress_listing();
    let encoded = codec::encode(&listing);
    let mut submitted_bytes = 0_u64;
    let locations: Vec<Location> = (0..50_000)
        .map(|index| Location::new(format!("other://stress/folder-{index:05}")).unwrap())
        .collect();
    for batch in locations.chunks(16) {
        for location in batch {
            let key = keys.key(location).unwrap();
            submitted_bytes += (codec::encode_key(&key).len() + encoded.len()) as u64;
            cache.put(key, listing.clone());
        }
        disk.save_folders(&cache, &keys, batch);
        assert!(disk.flush(deadline.saturating_duration_since(Instant::now())));
        assert!(disk.stats().bytes <= DISK_BUDGET);
        for at in batch {
            cache.invalidate(&keys.key(at).unwrap());
        }
    }
    let newest = keys.key(locations.last().unwrap()).unwrap();
    assert_eq!(
        db.get("listing", &codec::encode_key(&newest))
            .unwrap()
            .as_ref(),
        Some(&encoded)
    );
    let stats = disk.stats();
    let usage = db.usage("listing").unwrap();
    assert!(submitted_bytes > DISK_BUDGET);
    assert_eq!(stats.budget, DISK_BUDGET);
    assert!(usage.0 > 0 && usage.0 <= stats.budget, "{usage:?}");
    assert!(usage.1 > 0 && usage.1 < 50_000, "{usage:?}");
    assert_eq!((stats.bytes, stats.entries), usage);
    assert!(disk.close(deadline.saturating_duration_since(Instant::now())));
}
