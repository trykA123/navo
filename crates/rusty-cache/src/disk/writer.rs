use std::collections::{HashMap, HashSet};

use rusty_core::Location;

use super::ListingTable;
use super::codec::Body;
use crate::{CacheKey, CachedListing};

pub(super) struct Writer {
    table: ListingTable,
    written: HashMap<CacheKey, u32>,
}

impl Writer {
    pub(super) fn new(table: ListingTable) -> Self {
        Self {
            table,
            written: HashMap::new(),
        }
    }

    pub(super) fn write(&mut self, key: &CacheKey, listing: &CachedListing, is_forced: bool) {
        let body = Body::new(listing);
        let digest = body.rows_digest();
        if !is_forced && self.written.get(key) == Some(&digest) {
            return;
        }
        match self.table.save_body(key, &body) {
            Ok(()) => {
                self.written.insert(key.clone(), digest);
            }
            Err(error) => tracing::warn!(%error, "listing cache write failed"),
        }
    }

    pub(super) fn usage(&self) -> (u64, u64) {
        self.table.usage()
    }

    pub(super) fn forget_closed(&mut self, open: &HashSet<Location>) {
        self.written.retain(|key, _| open.contains(&key.location));
    }
}
