use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::anchor::SectionPair;
use crate::model::{Budget, CancelToken, DiffStep};
use crate::text::diff::Hunk;
use crate::text::digest::HUNK_DIGEST_ENTRY_BYTES;
use crate::text::scan::Candidate;

use super::plan::Hook;
use crate::text::side::Line;

use super::read::Lines;

mod hash;
mod out;
mod sections;
mod store;
mod work;

pub(crate) use hash::hash_hunks;
pub(crate) use out::{Kind, Out, Part};
pub(crate) use sections::{Planned, plan_sections};
pub use store::Evictions;
pub(crate) use store::Recorded;
pub(crate) use store::Store;
pub(crate) use store::StoreSeed;
pub use work::Rediff;
pub(crate) use work::{
    Ceiling, Detail, Hunks, coarse_batch, detail, diff_slices, ladder, over_cap, read_lines, rediff,
};

pub(crate) const MAX_EXTRA_WORKERS: u32 = 2;
pub(super) const WATCHDOG: Duration = Duration::from_secs(2);
pub(super) const WATCH_POLL: Duration = Duration::from_millis(10);
pub(super) const HUNK_SIZE: u64 = size_of::<Hunk>() as u64;
pub(super) const DIGEST_SIZE: u64 = HUNK_DIGEST_ENTRY_BYTES;
const HUNK_BUDGET_DIVISOR: u64 = 4;
const FOCUS_BATCHES: u64 = 4;
const CANDIDATE_SHARE: u64 = 8;
const LINE_WORK: u64 = 320;
const LINE_HELD: u64 = 80;
const TEXT_FACTOR: u64 = 2;
/// The raw bytes a `Lines` keeps for hashing, one copy of the section.
const RAW_FACTOR: u64 = 1;
const WORK_FIXED: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepReason {
    SectionCap,
    TokenCap,
    Budget,
    Watchdog,
    Paged,
}

/// Whether `hunks_for` can diff a counted section again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagedState {
    /// The store kept the section, so `hunks_for` can re-diff it.
    Rediffable,
    /// The store had no room for its metadata; only the count remains.
    CountsOnly,
}

/// A section kept as a count; `hunks_for(left)` re-diffs it when rediffable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paged {
    pub left: Range<u64>,
    pub right: Range<u64>,
    pub hunks: u64,
    pub state: PagedState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StepInfo {
    pub(super) step: DiffStep,
    pub(super) section: Range<u64>,
    pub(super) lines: u64,
    pub(super) reason: StepReason,
}

impl StepInfo {
    pub(super) fn new(pair: &SectionPair, reason: StepReason) -> Self {
        let step = if reason == StepReason::Paged {
            DiffStep::PagedHunks
        } else {
            DiffStep::Coarse
        };
        Self {
            step,
            section: pair.left.clone(),
            lines: span(&pair.left).saturating_add(span(&pair.right)),
            reason,
        }
    }
}

pub(crate) type Stall = Arc<dyn Fn(u64) + Send + Sync>;

pub(super) struct Limits {
    pub(super) ledger: Arc<Ledger>,
    pub(super) focus: Arc<AtomicU64>,
    pub(super) paging: Arc<AtomicBool>,
    pub(super) cancel: CancelToken,
    pub(super) stall: Option<Stall>,
    pub(super) expired: Option<super::schedule::Finished>,
    pub(super) before_hash: Option<Hook>,
    pub(super) watchdog: Duration,
    pub(super) reach: u64,
}

impl Limits {
    pub(super) fn distant(&self, left: &Range<u64>) -> bool {
        self.paging.load(Ordering::Acquire)
            && !near(self.focus.load(Ordering::Relaxed), self.reach, left)
    }
}

pub(super) fn near(focus: u64, reach: u64, left: &Range<u64>) -> bool {
    left.start <= focus.saturating_add(reach) && left.end.saturating_add(reach) >= focus
}

pub(super) fn reach(batch_lines: u64) -> u64 {
    batch_lines.saturating_mul(FOCUS_BATCHES)
}

/// Counts the engine's working memory for one diff against its budget.
pub(crate) struct Ledger {
    limit: u64,
    used: AtomicU64,
    peak: AtomicU64,
    wake: AtomicBool,
}

impl Ledger {
    pub(super) fn new(budget: Budget) -> Self {
        Self {
            limit: budget.bytes(),
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            wake: AtomicBool::new(false),
        }
    }

    pub(super) fn wake(&self) {
        self.wake.store(true, Ordering::Release);
    }

    pub(super) fn take_wake(&self) -> bool {
        self.wake.swap(false, Ordering::AcqRel)
    }

    pub(super) fn available(&self) -> u64 {
        self.limit.saturating_sub(self.used())
    }

    pub(super) fn hunk_share(&self) -> u64 {
        self.limit / HUNK_BUDGET_DIVISOR
    }

    pub(super) fn candidate_cap(&self) -> usize {
        let share = self.limit / CANDIDATE_SHARE / size_of::<Candidate>() as u64;
        usize::try_from(share).unwrap_or(usize::MAX)
    }

    /// Reserves `bytes` only when they fit under the limit.
    pub(super) fn reserve(self: &Arc<Self>, bytes: u64) -> Option<Charge> {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let next = used.checked_add(bytes).filter(|&next| next <= self.limit)?;
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.peak.fetch_max(next, Ordering::AcqRel);
                    return Some(Charge {
                        ledger: Arc::clone(self),
                        bytes,
                    });
                }
                Err(current) => used = current,
            }
        }
    }

    pub(super) fn empty(self: &Arc<Self>) -> Charge {
        Charge {
            ledger: Arc::clone(self),
            bytes: 0,
        }
    }

    fn grow(&self, bytes: u64) {
        let next = self
            .used
            .fetch_add(bytes, Ordering::AcqRel)
            .saturating_add(bytes);
        self.peak.fetch_max(next, Ordering::AcqRel);
    }

    fn shrink(&self, bytes: u64) {
        self.used.fetch_sub(bytes, Ordering::AcqRel);
    }

    pub(crate) fn used(&self) -> u64 {
        self.used.load(Ordering::Acquire)
    }

    pub(crate) fn peak(&self) -> u64 {
        self.peak.load(Ordering::Acquire)
    }
}

/// Bytes held against the ledger; dropping it gives them back.
pub(crate) struct Charge {
    ledger: Arc<Ledger>,
    bytes: u64,
}

impl Charge {
    pub(super) fn wake(&self) {
        self.ledger.wake();
    }

    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(super) fn fresh(&self) -> Self {
        Self {
            ledger: Arc::clone(&self.ledger),
            bytes: 0,
        }
    }

    /// Splits off exactly `bytes`, counting any part this charge did not hold.
    pub(super) fn take(&mut self, bytes: u64) -> Self {
        let mut part = self.split(bytes);
        part.settle(bytes);
        part
    }

    /// Adds `bytes` only when they fit under the limit.
    pub(super) fn try_add(&mut self, bytes: u64) -> bool {
        match self.ledger.reserve(bytes) {
            Some(added) => {
                self.absorb(added);
                true
            }
            None => false,
        }
    }

    /// Matches the measured size; growth is always counted, so the peak stays honest.
    pub(super) fn settle(&mut self, actual: u64) {
        if actual > self.bytes {
            self.ledger.grow(actual - self.bytes);
        } else {
            self.ledger.shrink(self.bytes - actual);
        }
        self.bytes = actual;
    }

    pub(super) fn absorb(&mut self, mut other: Self) {
        self.bytes += std::mem::take(&mut other.bytes);
    }

    pub(super) fn split(&mut self, bytes: u64) -> Self {
        let moved = bytes.min(self.bytes);
        self.bytes -= moved;
        Self {
            ledger: Arc::clone(&self.ledger),
            bytes: moved,
        }
    }

    /// Counts lines that read larger than the estimate allowed for them.
    pub(super) fn cover_lines(&mut self, lines: u64, bytes: u64, measured: u64) {
        let allowance = lines
            .saturating_mul(LINE_HELD)
            .saturating_add(byte_work(bytes));
        if measured > allowance {
            self.settle(self.bytes.saturating_add(measured - allowance));
        }
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        if self.bytes > 0 {
            self.ledger.shrink(self.bytes);
        }
    }
}

/// Makes room for `more` items, charging the new capacity first; false when it does not fit.
pub(super) fn grow_vec<T>(items: &mut Vec<T>, more: usize, charge: &mut Charge) -> bool {
    let need = items.len().saturating_add(more);
    if need <= items.capacity() {
        return true;
    }
    let before = items.capacity();
    let target = need.max(before.saturating_mul(2)).max(16);
    let added = bytes_of::<T>(target - before);
    if !charge.try_add(added) {
        return false;
    }
    items.reserve_exact(target - items.len());
    let actual = bytes_of::<T>(items.capacity() - before);
    charge.settle(charge.bytes() - added + actual);
    true
}

pub(super) fn bytes_of<T>(count: usize) -> u64 {
    u64::try_from(count)
        .unwrap_or(u64::MAX)
        .saturating_mul(size_of::<T>() as u64)
}

pub(super) fn lines_bytes(lines: &Lines) -> u64 {
    let text: usize = lines.lines.iter().map(|line| line.text.capacity()).sum();
    bytes_of::<Line>(lines.lines.capacity())
        .saturating_add(bytes_of::<u64>(lines.starts.capacity()))
        .saturating_add(u64::try_from(text).unwrap_or(u64::MAX))
        .saturating_add(raw_bytes(lines))
}

/// Upper bound of one section diff: decoded lines, key hashes, the Histogram
/// input and interner, its occurrence lists, the rows, the hunks and the raw
/// bytes kept for the digests.
pub(super) fn work_estimate(lines: u64, bytes: u64) -> u64 {
    lines
        .saturating_mul(LINE_WORK)
        .saturating_add(byte_work(bytes))
        .saturating_add(WORK_FIXED)
}

fn byte_work(bytes: u64) -> u64 {
    bytes.saturating_mul(TEXT_FACTOR + RAW_FACTOR)
}

/// Bytes the section's raw copy holds; freed once its digests exist.
pub(super) fn raw_bytes(lines: &Lines) -> u64 {
    u64::try_from(lines.bytes.capacity()).unwrap_or(u64::MAX)
}

pub(super) fn span(range: &Range<u64>) -> u64 {
    range.end - range.start
}
