use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use crate::anchor::SectionPair;
use crate::model::Reason;
use crate::text::diff::Hunk;

use super::super::LargeDiff;
use super::super::batch::Context;
use super::{
    Ceiling, Charge, HUNK_SIZE, Kind, Limits, Paged, PagedState, Part, bytes_of, grow_vec, near,
    rediff,
};

struct Entry {
    pair: SectionPair,
    hunks: u64,
    detailed: bool,
    spare: u64,
}

struct Held {
    upto: u64,
    _charge: Charge,
}

/// Hunks the view has dropped; the charge lasts until the view applies them.
#[must_use]
pub struct Evictions {
    paged: Vec<Paged>,
    hold: Charge,
}

impl Evictions {
    pub(crate) fn empty(charge: Charge) -> Self {
        Self {
            paged: Vec::new(),
            hold: charge,
        }
    }

    /// Sections the view must drop before acknowledging.
    #[must_use]
    pub fn paged(&self) -> &[Paged] {
        &self.paged
    }

    /// Applies every eviction, then releases the memory they held.
    pub fn apply(self, mut sink: impl FnMut(&Paged)) {
        for paged in &self.paged {
            sink(paged);
        }
        self.acknowledge();
    }

    /// Call after the view has applied `paged`; until then the memory stays counted.
    pub fn acknowledge(self) {
        let ledger = Arc::clone(&self.hold.ledger);
        drop(self.hold);
        ledger.wake();
    }

    pub(crate) fn into_parts(self) -> (Vec<Paged>, Charge) {
        (self.paged, self.hold)
    }
}

pub(crate) struct StoreSeed {
    pub(crate) sections: usize,
    pub(crate) seq: Arc<std::sync::atomic::AtomicU64>,
}

/// What `Store::record` could keep for one batch.
pub(crate) enum Recorded {
    Evictions(Evictions),
    /// The store had no room for metadata; only counts are available.
    CountsOnly,
}

/// The emitted sections with their final pairs, and the hunk bytes the view holds for them.
pub(crate) struct Store {
    context: Arc<Context>,
    entries: Vec<Entry>,
    detailed: Vec<usize>,
    detailed_bytes: u64,
    entries_charge: Charge,
    hunks_charge: Charge,
    pending: Vec<Held>,
    seq: Arc<std::sync::atomic::AtomicU64>,
    spare_bytes: u64,
    paging: Arc<AtomicBool>,
    share: u64,
    reach: u64,
    watchdog: Duration,
    focus: u64,
}

impl Store {
    pub(crate) fn new(
        context: Arc<Context>,
        charge: Charge,
        limits: &Limits,
        seed: StoreSeed,
    ) -> Self {
        let StoreSeed { sections, seq } = seed;
        let mut store = Self {
            context,
            entries: Vec::new(),
            detailed: Vec::new(),
            detailed_bytes: 0,
            hunks_charge: limits.ledger.empty(),
            pending: Vec::new(),
            seq,
            spare_bytes: 0,
            entries_charge: charge,
            paging: Arc::clone(&limits.paging),
            share: limits.ledger.hunk_share(),
            reach: limits.reach,
            watchdog: limits.watchdog,
            focus: u64::MAX,
        };
        store.entries.reserve_exact(sections);
        store.detailed.reserve_exact(sections);
        store.count();
        store
    }

    /// Bytes one section takes in the store at most: its entry and its detailed slot.
    pub(crate) fn section_bytes() -> u64 {
        bytes_of::<Entry>(1).saturating_add(bytes_of::<usize>(1))
    }

    /// Records emitted parts; counts-only when the metadata cannot be reserved.
    pub(crate) fn record(&mut self, parts: &[Part], focus: u64, hunk_capacity: usize) -> Recorded {
        let detailed = parts
            .iter()
            .filter(|part| part.kind == Kind::Detail)
            .count();
        if !grow_vec(&mut self.entries, parts.len(), &mut self.entries_charge)
            || !grow_vec(&mut self.detailed, detailed, &mut self.entries_charge)
        {
            return Recorded::CountsOnly;
        }
        let len = parts.last().map_or(0, |part| part.end);
        let spare = bytes_of::<Hunk>(hunk_capacity.saturating_sub(len));
        let mut start = 0;
        for (at, part) in parts.iter().enumerate() {
            let count = u64::try_from(part.end - start).unwrap_or(u64::MAX);
            start = part.end;
            let spare = if at + 1 == parts.len() { spare } else { 0 };
            let (hunks, detailed) = match part.kind {
                Kind::Detail => (count, true),
                Kind::Paged(hunks) => (hunks, false),
                Kind::Coarse => (1, false),
            };
            if detailed {
                self.detailed.push(self.entries.len());
                self.detailed_bytes = self.detailed_bytes.saturating_add(count * HUNK_SIZE);
            }
            self.spare_bytes = self.spare_bytes.saturating_add(spare);
            self.entries.push(Entry {
                pair: part.pair.clone(),
                hunks,
                detailed,
                spare,
            });
        }
        self.count();
        if self.detailed_bytes.saturating_add(self.spare_bytes) > self.share {
            self.paging.store(true, Ordering::Release);
        }
        Recorded::Evictions(self.page(focus))
    }

    /// Drops distant sections to counts once paging is on.
    pub(crate) fn page(&mut self, focus: u64) -> Evictions {
        self.focus = focus;
        if !self.paging.load(Ordering::Acquire) {
            return Evictions::empty(self.hunks_charge.fresh());
        }
        let mut count = 0;
        let mut released = 0_u64;
        let mut spare = 0_u64;
        for &index in &self.detailed {
            let entry = &self.entries[index];
            if near(focus, self.reach, &entry.pair.left) {
                continue;
            }
            count += 1;
            released = released.saturating_add(entry.hunks.saturating_sub(1) * HUNK_SIZE);
            spare = spare.saturating_add(entry.spare);
        }
        if count == 0 {
            return Evictions::empty(self.hunks_charge.fresh());
        }
        let keep_len = self.detailed.len() - count;
        let need = bytes_of::<Paged>(count).saturating_add(bytes_of::<usize>(keep_len));
        let mut hold = self.hunks_charge.fresh();
        if !hold.try_add(need) {
            return Evictions::empty(self.hunks_charge.fresh());
        }
        hold.absorb(self.hunks_charge.split(released.saturating_add(spare)));
        let (paged, keep) = self.evict(focus, count, keep_len);
        self.detailed = keep;
        self.detailed_bytes = self.detailed_bytes.saturating_sub(released);
        self.count();
        Evictions { paged, hold }
    }

    fn evict(&mut self, focus: u64, count: usize, keep_len: usize) -> (Vec<Paged>, Vec<usize>) {
        let mut paged = Vec::with_capacity(count);
        let mut keep = Vec::with_capacity(keep_len);
        for &index in &self.detailed {
            let entry = &mut self.entries[index];
            if near(focus, self.reach, &entry.pair.left) {
                keep.push(index);
                continue;
            }
            entry.detailed = false;
            let spare = std::mem::take(&mut entry.spare);
            self.spare_bytes = self.spare_bytes.saturating_sub(spare);
            paged.push(Paged {
                left: entry.pair.left.clone(),
                right: entry.pair.right.clone(),
                hunks: entry.hunks,
                state: PagedState::Rediffable,
            });
        }
        (paged, keep)
    }

    /// Stores a deferred eviction charge under the sequence of its last event;
    /// sequences stay monotonic for the whole diff, including restarts.
    pub(crate) fn defer(&mut self, evictions: Evictions) -> (Vec<Paged>, u64) {
        let (paged, hold) = evictions.into_parts();
        let count = u64::try_from(paged.len()).unwrap_or(u64::MAX);
        let first = self
            .seq
            .fetch_add(count, Ordering::AcqRel)
            .saturating_add(1);
        if paged.is_empty() {
            drop(hold);
        } else {
            self.pending.push(Held {
                upto: first.saturating_add(count).saturating_sub(1),
                _charge: hold,
            });
        }
        (paged, first)
    }

    /// Releases every deferred charge whose last event is at or below `upto`.
    pub(crate) fn acknowledge(&mut self, upto: u64) {
        self.pending.retain(|held| held.upto > upto);
        self.hunks_charge.wake();
    }

    fn count(&mut self) {
        let blocks = self.entries.len() - self.detailed.len();
        let held = self
            .detailed_bytes
            .saturating_add(self.spare_bytes)
            .saturating_add(bytes_of::<Hunk>(blocks))
            .saturating_add(bytes_of::<Paged>(blocks));
        self.hunks_charge.settle(held);
        let capacity = bytes_of::<Entry>(self.entries.capacity())
            .saturating_add(bytes_of::<usize>(self.detailed.capacity()));
        self.entries_charge
            .settle(capacity.max(self.entries_charge.bytes()));
    }

    fn find(&self, left: &Range<u64>) -> Option<SectionPair> {
        self.entries
            .iter()
            .find(|entry| entry.pair.left == *left)
            .map(|entry| entry.pair.clone())
    }
}

impl LargeDiff {
    /// Line hunks of one emitted section, found by its exact left range.
    pub fn hunks_for(&self, section: Range<u64>) -> Result<super::Rediff, Reason> {
        if self.cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let (context, pair, watchdog) = {
            let store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
            let store = store.as_ref().ok_or(Reason::Unsupported)?;
            let pair = store.find(&section).ok_or(Reason::Unsupported)?;
            (Arc::clone(&store.context), pair, store.watchdog)
        };
        rediff(
            &context,
            &pair,
            &self.ledger,
            Ceiling {
                cancel: &self.cancel,
                watchdog,
            },
        )
    }

    #[must_use]
    pub fn memory_used(&self) -> u64 {
        self.ledger.used()
    }

    #[must_use]
    pub fn peak_memory(&self) -> u64 {
        self.ledger.peak()
    }

    /// Releases streamed eviction charges up to the highest applied event sequence.
    /// Call only after applying every `Paged` event with `seq <= upto`.
    pub fn acknowledge_evictions(&self, upto: u64) {
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(store) = store.as_mut() {
            store.acknowledge(upto);
        }
    }
}

#[cfg(test)]
mod tests;
