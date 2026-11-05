use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::listing::NameLookup;
use crate::{CacheKey, CacheStats, CachedListing};

struct Slot {
    listing: Arc<CachedListing>,
    lookup: Option<NameLookup>,
    bytes: usize,
    tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Swap {
    Done,
    Missing,
    Stale,
}

pub(crate) struct SizedListing {
    pub(crate) listing: Arc<CachedListing>,
    pub(crate) lookup: Option<NameLookup>,
    pub(crate) bytes: usize,
}

impl SizedListing {
    pub(crate) fn new(key: &CacheKey, listing: CachedListing) -> Self {
        Self::with_lookup(key, listing, None)
    }

    pub(crate) fn with_lookup(
        key: &CacheKey,
        listing: CachedListing,
        lookup: Option<NameLookup>,
    ) -> Self {
        let lookup_bytes = lookup.as_ref().map_or(0, NameLookup::bytes);
        let bytes = key.estimated_bytes() + listing.estimated_bytes() + lookup_bytes;
        Self {
            listing: Arc::new(listing),
            lookup,
            bytes,
        }
    }
}

pub(crate) struct Store {
    budget: usize,
    slots: HashMap<CacheKey, Slot>,
    order: BTreeMap<u64, CacheKey>,
    pins: HashMap<CacheKey, usize>,
    bytes: usize,
    pinned_bytes: usize,
    tick: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Store {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            budget,
            slots: HashMap::new(),
            order: BTreeMap::new(),
            pins: HashMap::new(),
            bytes: 0,
            pinned_bytes: 0,
            tick: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    pub(crate) fn get(&mut self, key: &CacheKey) -> Option<Arc<CachedListing>> {
        let tick = self.next_tick();
        let Some(slot) = self.slots.get_mut(key) else {
            self.misses += 1;
            return None;
        };
        self.order.remove(&slot.tick);
        slot.tick = tick;
        self.order.insert(tick, key.clone());
        self.hits += 1;
        Some(Arc::clone(&slot.listing))
    }

    pub(crate) fn peek(&self, key: &CacheKey) -> Option<Arc<CachedListing>> {
        self.slots.get(key).map(|slot| Arc::clone(&slot.listing))
    }

    // The slot keeps its listing; only the lookup leaves, so a stale swap cannot move it to new rows.
    pub(crate) fn take_lookup(
        &mut self,
        key: &CacheKey,
    ) -> Option<(Arc<CachedListing>, Option<NameLookup>)> {
        let (listing, lookup, bytes) = {
            let slot = self.slots.get_mut(key)?;
            let lookup = slot.lookup.take();
            let bytes = lookup.as_ref().map_or(0, NameLookup::bytes);
            slot.bytes -= bytes;
            (Arc::clone(&slot.listing), lookup, bytes)
        };
        self.bytes -= bytes;
        if self.is_pinned(key) {
            self.pinned_bytes -= bytes;
        }
        Some((listing, lookup))
    }

    pub(crate) fn restore_lookup(
        &mut self,
        key: &CacheKey,
        seen: &Arc<CachedListing>,
        lookup: Option<NameLookup>,
    ) {
        let Some(lookup) = lookup else {
            return;
        };
        let pinned = self.is_pinned(key);
        let Some(slot) = self.slots.get_mut(key) else {
            return;
        };
        if !Arc::ptr_eq(&slot.listing, seen) {
            return;
        }
        let bytes = lookup.bytes();
        slot.lookup = Some(lookup);
        slot.bytes += bytes;
        self.bytes += bytes;
        if pinned {
            self.pinned_bytes += bytes;
        }
        self.evict_to_budget();
    }

    #[cfg(test)]
    pub(crate) fn lookup(&self, key: &CacheKey) -> Option<&NameLookup> {
        self.slots.get(key)?.lookup.as_ref()
    }

    pub(crate) fn put(&mut self, key: CacheKey, entry: SizedListing) {
        if self
            .slots
            .get(&key)
            .is_some_and(|slot| entry.listing.read_at < slot.listing.read_at)
        {
            return;
        }
        self.remove(&key);
        if entry.bytes > self.budget {
            return;
        }
        if !self.is_pinned(&key) && self.pinned_bytes + entry.bytes > self.budget {
            return;
        }
        let tick = self.next_tick();
        self.insert(key, entry, tick);
        self.evict_to_budget();
    }

    pub(crate) fn swap(
        &mut self,
        key: &CacheKey,
        seen: &Arc<CachedListing>,
        entry: SizedListing,
    ) -> Swap {
        let Some(slot) = self.slots.get(key) else {
            return Swap::Missing;
        };
        if !Arc::ptr_eq(&slot.listing, seen) {
            return Swap::Stale;
        }
        let tick = slot.tick;
        self.remove(key);
        if entry.bytes <= self.budget {
            self.insert(key.clone(), entry, tick);
            self.evict_to_budget();
        }
        Swap::Done
    }

    pub(crate) fn invalidate(&mut self, key: &CacheKey) {
        self.remove(key);
    }

    pub(crate) fn pin(&mut self, key: &CacheKey) {
        let count = self.pins.entry(key.clone()).or_default();
        *count += 1;
        if *count == 1 {
            self.pinned_bytes += self.slots.get(key).map_or(0, |slot| slot.bytes);
        }
    }

    pub(crate) fn unpin(&mut self, key: &CacheKey) {
        let Some(count) = self.pins.get_mut(key) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.pins.remove(key);
            self.pinned_bytes -= self.slots.get(key).map_or(0, |slot| slot.bytes);
            self.evict_to_budget();
        }
    }

    pub(crate) fn stats(&self) -> CacheStats {
        CacheStats {
            bytes: self.bytes,
            pinned_bytes: self.pinned_bytes,
            entries: self.slots.len(),
            pinned_entries: self
                .pins
                .keys()
                .filter(|key| self.slots.contains_key(*key))
                .count(),
            budget: self.budget,
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
        }
    }

    pub(crate) fn pinned_bytes(&self) -> usize {
        self.pinned_bytes
    }

    #[cfg(test)]
    pub(crate) fn scanned_pinned_bytes(&self) -> usize {
        self.slots
            .iter()
            .filter(|(key, _)| self.is_pinned(key))
            .map(|(_, slot)| slot.bytes)
            .sum()
    }

    fn is_pinned(&self, key: &CacheKey) -> bool {
        self.pins.contains_key(key)
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    fn insert(&mut self, key: CacheKey, entry: SizedListing, tick: u64) {
        self.order.insert(tick, key.clone());
        self.bytes += entry.bytes;
        if self.is_pinned(&key) {
            self.pinned_bytes += entry.bytes;
        }
        let SizedListing {
            listing,
            lookup,
            bytes,
        } = entry;
        self.slots.insert(
            key,
            Slot {
                listing,
                lookup,
                bytes,
                tick,
            },
        );
    }

    fn remove(&mut self, key: &CacheKey) -> bool {
        let Some(slot) = self.slots.remove(key) else {
            return false;
        };
        self.order.remove(&slot.tick);
        self.bytes -= slot.bytes;
        if self.is_pinned(key) {
            self.pinned_bytes -= slot.bytes;
        }
        true
    }

    fn evict_to_budget(&mut self) {
        while self.bytes > self.budget {
            let Some(victim) = self.oldest_unpinned() else {
                return;
            };
            if self.remove(&victim) {
                self.evictions += 1;
            }
        }
    }

    fn oldest_unpinned(&self) -> Option<CacheKey> {
        self.order
            .values()
            .find(|key| !self.is_pinned(key))
            .cloned()
    }
}
