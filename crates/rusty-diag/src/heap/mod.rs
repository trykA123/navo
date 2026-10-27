use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HeapStats {
    pub live_bytes: u64,
    pub peak_bytes: u64,
    pub allocations: u64,
}

#[derive(Debug, Default)]
pub struct HeapCounter {
    live: AtomicUsize,
    peak: AtomicUsize,
    allocations: AtomicU64,
}

impl HeapCounter {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            allocations: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn allocated(&self, size: usize) {
        self.allocations.fetch_add(1, Relaxed);
        self.grow(size);
    }

    #[inline]
    pub fn freed(&self, size: usize) {
        self.live.fetch_sub(size, Relaxed);
    }

    #[inline]
    pub fn reallocated(&self, old: usize, new: usize) {
        self.allocations.fetch_add(1, Relaxed);
        if new >= old {
            self.grow(new - old);
        } else {
            self.freed(old - new);
        }
    }

    #[inline]
    fn grow(&self, size: usize) {
        let live = self.live.fetch_add(size, Relaxed).wrapping_add(size);
        if live > self.peak.load(Relaxed) {
            self.peak.fetch_max(live, Relaxed);
        }
    }

    #[must_use]
    pub fn stats(&self) -> HeapStats {
        let live = self.live.load(Relaxed);
        let live = if live > isize::MAX as usize { 0 } else { live };
        HeapStats {
            live_bytes: live as u64,
            peak_bytes: self.peak.load(Relaxed) as u64,
            allocations: self.allocations.load(Relaxed),
        }
    }
}

static READER: OnceLock<fn() -> HeapStats> = OnceLock::new();

pub fn set_reader(read: fn() -> HeapStats) {
    let _ = READER.set(read);
}

#[must_use]
pub fn stats() -> Option<HeapStats> {
    READER.get().map(|read| read())
}

#[cfg(test)]
mod tests;
