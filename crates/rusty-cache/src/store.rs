use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::{CacheKey, CacheStats, CachedListing, Change};

struct Slot {
    listing: Arc<CachedListing>,
    bytes: usize,
    tick: u64,
}

pub(crate) struct Store {
    budget: usize,
    slots: HashMap<CacheKey, Slot>,
    order: BTreeMap<u64, CacheKey>,
    pins: HashMap<CacheKey, usize>,
    bytes: usize,
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

    pub(crate) fn put(&mut self, key: CacheKey, listing: CachedListing) {
        self.remove(&key);
        let bytes = key.estimated_bytes() + listing.estimated_bytes();
        if bytes > self.budget {
            return;
        }
        if !self.is_pinned(&key) && self.pinned_bytes() + bytes > self.budget {
            return;
        }
        self.insert(key, Arc::new(listing), bytes);
        self.evict_to_budget();
    }

    pub(crate) fn patch(&mut self, key: &CacheKey, changes: impl IntoIterator<Item = Change>) {
        let Some(slot) = self.slots.get(key) else {
            return;
        };
        let listing = slot.listing.patched(changes);
        let tick = slot.tick;
        self.remove(key);
        let bytes = key.estimated_bytes() + listing.estimated_bytes();
        if bytes > self.budget {
            return;
        }
        let listing = Arc::new(listing);
        self.insert_at(
            key.clone(),
            Slot {
                listing,
                bytes,
                tick,
            },
        );
        self.evict_to_budget();
    }

    pub(crate) fn invalidate(&mut self, key: &CacheKey) {
        self.remove(key);
    }

    pub(crate) fn pin(&mut self, key: &CacheKey) {
        *self.pins.entry(key.clone()).or_default() += 1;
    }

    pub(crate) fn unpin(&mut self, key: &CacheKey) {
        let Some(count) = self.pins.get_mut(key) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.pins.remove(key);
            self.evict_to_budget();
        }
    }

    pub(crate) fn stats(&self) -> CacheStats {
        CacheStats {
            bytes: self.bytes,
            entries: self.slots.len(),
            budget: self.budget,
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
        }
    }

    pub(crate) fn pinned_bytes(&self) -> usize {
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

    fn insert(&mut self, key: CacheKey, listing: Arc<CachedListing>, bytes: usize) {
        let tick = self.next_tick();
        self.insert_at(
            key,
            Slot {
                listing,
                bytes,
                tick,
            },
        );
    }

    fn insert_at(&mut self, key: CacheKey, slot: Slot) {
        self.order.insert(slot.tick, key.clone());
        self.bytes += slot.bytes;
        self.slots.insert(key, slot);
    }

    fn remove(&mut self, key: &CacheKey) -> bool {
        let Some(slot) = self.slots.remove(key) else {
            return false;
        };
        self.order.remove(&slot.tick);
        self.bytes -= slot.bytes;
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
