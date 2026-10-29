use std::alloc::{GlobalAlloc, Layout};

#[cfg(feature = "mimalloc")]
use mimalloc::MiMalloc;
#[cfg(not(feature = "mimalloc"))]
use std::alloc::System;

use rusty_diag::heap::{HeapCounter, HeapStats};

static COUNTER: HeapCounter = HeapCounter::new();

#[cfg(feature = "mimalloc")]
static INNER: MiMalloc = MiMalloc;
#[cfg(not(feature = "mimalloc"))]
static INNER: System = System;

struct CountingAlloc;

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

// SAFETY: every call forwards to the inner allocator with the caller's layout and pointer unchanged; the counter only does relaxed atomic arithmetic and never allocates.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`.
        let block = unsafe { INNER.alloc(layout) };
        if !block.is_null() {
            COUNTER.allocated(layout.size());
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract for `layout`.
        let block = unsafe { INNER.alloc_zeroed(layout) };
        if !block.is_null() {
            COUNTER.allocated(layout.size());
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: `block` came from this allocator, which got it from the inner allocator with the same `layout`.
        unsafe { INNER.dealloc(block, layout) };
        COUNTER.freed(layout.size());
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `block` came from the inner allocator with `layout`, and the caller upholds `realloc`'s contract for `new_size`.
        let moved = unsafe { INNER.realloc(block, layout, new_size) };
        if !moved.is_null() {
            COUNTER.reallocated(layout.size(), new_size);
        }
        moved
    }
}

pub fn install() {
    rusty_diag::heap::set_reader(stats);
}

fn stats() -> HeapStats {
    COUNTER.stats()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_installed_allocator_counts_a_live_buffer() {
        super::install();
        let before = super::stats();
        let buffer = vec![7_u8; 4 << 20];
        let during = rusty_diag::heap::stats().unwrap();
        drop(buffer);
        assert!(during.allocations > before.allocations);
        assert!(during.live_bytes >= 4 << 20);
        assert!(during.peak_bytes >= during.live_bytes);
    }
}
