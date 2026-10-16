use std::time::Instant;

use rusty_core::Location;

use super::ListingTable;
use crate::keys::Lookup;
use crate::{CacheKey, CacheKeys, ListingCache};

pub struct Warm {
    pub cache: ListingCache,
    pub keys: CacheKeys,
    pub folders: Vec<Location>,
    pub deadline: Instant,
}

pub(super) fn warm_up(mut table: ListingTable, warm: Warm) -> ListingTable {
    let Warm {
        cache,
        keys,
        mut folders,
        deadline,
    } = warm;
    for location in &folders {
        let _ = keys.key(location);
    }
    loop {
        let answers = keys.answers();
        folders.retain(|location| match keys.lookup(location) {
            Lookup::Found(key) => {
                admit(&mut table, &cache, key, deadline);
                false
            }
            Lookup::Probing => true,
            Lookup::Missing => false,
        });
        if folders.is_empty() || !keys.wait_for_answer(answers, deadline) {
            return table;
        }
    }
}

pub(crate) fn admit(
    table: &mut ListingTable,
    cache: &ListingCache,
    key: CacheKey,
    deadline: Instant,
) {
    if Instant::now() >= deadline {
        return;
    }
    let Some(mut listing) = table.load(&key) else {
        return;
    };
    listing.folder_changed = None;
    if Instant::now() < deadline {
        cache.put_absent(key, listing);
    }
}
