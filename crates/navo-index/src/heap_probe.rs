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
    // Bytes this thread allocated minus bytes it freed; other test threads never touch it.
    static THREAD_LIVE: Cell<isize> = const { Cell::new(0) };
}
static MEASURING: Mutex<()> = Mutex::new(());

struct Counting;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Adds `size` bytes to the global and per-thread counters and raises the global peak.
fn grow(size: usize) {
    THREAD_LIVE.set(THREAD_LIVE.get() + size as isize);
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
}

/// Removes `size` freed bytes from the global and per-thread counters.
fn shrink(size: usize) {
    THREAD_LIVE.set(THREAD_LIVE.get() - size as isize);
    LIVE.fetch_sub(size, Relaxed);
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
        shrink(layout.size());
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
            shrink(layout.size());
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
    thread_start: isize,
    _only: MutexGuard<'static, ()>,
}

impl Probe {
    pub fn start() -> Self {
        let only = MEASURING.lock().unwrap_or_else(PoisonError::into_inner);
        let start = LIVE.load(Relaxed);
        PEAK.store(start, Relaxed);
        Self {
            start,
            thread_start: THREAD_LIVE.get(),
            _only: only,
        }
    }

    pub fn read(&self) -> Measured {
        let live = LIVE.load(Relaxed);
        Measured {
            peak: PEAK.load(Relaxed) - self.start,
            retained: live as isize - self.start as isize,
        }
    }

    /// Bytes the calling thread holds now beyond what it held at `start`; blind to other threads.
    pub fn thread_retained(&self) -> isize {
        THREAD_LIVE.get() - self.thread_start
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

#[cfg(test)]
mod tests {
    use super::Probe;

    /// Another thread's allocation must not show in the measuring thread's retained bytes.
    #[test]
    fn thread_retained_ignores_other_threads() {
        let probe = Probe::start();
        let other = std::thread::spawn(|| vec![0u8; 1 << 20]);
        let held = other.join().unwrap();
        // The spawn itself leaves a few bytes of thread bookkeeping on this thread.
        let own = probe.thread_retained();
        assert!(
            own < 1024,
            "other thread's 1 MiB leaked into the count: {own}"
        );
        drop(held);
    }
}
