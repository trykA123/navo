use rusty_store::{CacheDb, CacheError};

use super::codec::{self, Body, DecodeError};
use crate::{CacheKey, CachedListing};

const TABLE: &str = "listing";

pub(crate) struct ListingTable {
    db: CacheDb,
}

impl ListingTable {
    pub(crate) fn new(mut db: CacheDb, budget: u64) -> Self {
        db.set_budget(TABLE, budget);
        Self { db }
    }

    pub(crate) fn load(&mut self, key: &CacheKey) -> Option<CachedListing> {
        let id = codec::encode_key(key);
        let bytes = match self.db.get(TABLE, &id) {
            Ok(bytes) => bytes?,
            Err(error) => {
                tracing::warn!(%error, "listing cache read failed");
                return None;
            }
        };
        match codec::decode(&bytes) {
            Ok(listing) => Some(listing),
            Err(error) => {
                self.drop_row(&id, error);
                None
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn save(
        &mut self,
        key: &CacheKey,
        listing: &CachedListing,
    ) -> Result<(), CacheError> {
        self.save_body(key, &Body::new(listing))
    }

    pub(crate) fn save_body(&mut self, key: &CacheKey, body: &Body) -> Result<(), CacheError> {
        self.db.put(TABLE, &codec::encode_key(key), &body.pack())
    }

    pub(crate) fn usage(&self) -> Option<(u64, u64)> {
        usage_of(&self.db)
    }

    fn drop_row(&mut self, id: &[u8], reason: DecodeError) {
        tracing::warn!(%reason, "dropping unreadable cached listing");
        if let Err(error) = self.db.remove(TABLE, id) {
            tracing::warn!(%error, "could not drop an unreadable cached listing");
        }
    }

    #[cfg(test)]
    pub(crate) fn used_bytes(&self) -> u64 {
        self.db.used_bytes(TABLE).unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn put_raw(&mut self, key: &CacheKey, value: &[u8]) {
        self.db
            .put(TABLE, &codec::encode_key(key), value)
            .unwrap_or_default();
    }
}

pub(crate) fn usage_of(db: &CacheDb) -> Option<(u64, u64)> {
    db.usage(TABLE)
        .inspect_err(|error| tracing::warn!(%error, "cannot measure the listing cache"))
        .ok()
}
