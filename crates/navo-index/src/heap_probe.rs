#![allow(
    unsafe_code,
    reason = "a counting global allocator for memory budget tests"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Mutex, MutexGuard, PoisonError};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
thread_local! {
    static FAIL_FROM: Cell<usize> = const { Cell::new(usize::MAX) };
}
static MEASURING: Mutex<()> = Mutex::new(());

struct Counting;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn grow(size: usize) {
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
}

// SAFETY: every call forwards to `System` with the caller's layout and pointer; only counters are added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let block = if layout.size() >= FAIL_FROM.get() {
            std::ptr::null_mut()
        } else {
            // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, passed on unchanged.
            unsafe { System.alloc(layout) }
        };
        if !block.is_null() {
            grow(layout.size());
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: `block` came from `System` through `alloc` or `realloc` with this layout.
        unsafe { System.dealloc(block, layout) };
        LIVE.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size >= FAIL_FROM.get() {
            return std::ptr::null_mut();
        }
        // SAFETY: `block` came from `System` with `layout`; the caller upholds `realloc`'s contract.
        let moved = unsafe { System.realloc(block, layout, new_size) };
        if !moved.is_null() {
            // A moving realloc holds both blocks for a moment; count it like the copy it may be.
            grow(new_size);
            LIVE.fetch_sub(layout.size(), Relaxed);
        }
        moved
    }
}

/// Heap bytes held and the highest point reached since the probe started, in bytes.
#[derive(Debug, Clone, Copy)]
pub struct Measured {
    pub peak: usize,
    pub retained: isize,
}

/// One measurement at a time; the guard keeps other memory tests from adding to the peak.
pub struct Probe {
    start: usize,
    _only: MutexGuard<'static, ()>,
}

impl Probe {
    pub fn start() -> Self {
        let only = MEASURING.lock().unwrap_or_else(PoisonError::into_inner);
        let start = LIVE.load(Relaxed);
        PEAK.store(start, Relaxed);
        Self { start, _only: only }
    }

    pub fn read(&self) -> Measured {
        let live = LIVE.load(Relaxed);
        Measured {
            peak: PEAK.load(Relaxed) - self.start,
            retained: live as isize - self.start as isize,
        }
    }

    pub fn mark(&self) -> usize {
        let live = LIVE.load(Relaxed);
        PEAK.store(live, Relaxed);
        live - self.start
    }
}

pub const MIB: usize = 1 << 20;

/// Makes this thread's allocations of `bytes` or more fail until the guard drops.
pub struct FailingAllocations;

pub fn fail_allocations_from(bytes: usize) -> FailingAllocations {
    FAIL_FROM.set(bytes);
    FailingAllocations
}

impl Drop for FailingAllocations {
    fn drop(&mut self) {
        FAIL_FROM.set(usize::MAX);
    }
}
