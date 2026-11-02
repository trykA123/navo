use std::sync::Arc;
use std::thread;

use super::*;

#[test]
fn alloc_then_free_returns_live_to_zero_and_keeps_the_peak() {
    let counter = HeapCounter::new();
    counter.allocated(100);
    counter.allocated(50);
    counter.freed(100);
    counter.freed(50);
    let stats = counter.stats();
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(stats.peak_bytes, 150);
    assert_eq!(stats.allocations, 2);
}

#[test]
fn realloc_moves_live_bytes_by_the_difference() {
    let counter = HeapCounter::new();
    counter.allocated(64);
    counter.reallocated(64, 256);
    assert_eq!(counter.stats().live_bytes, 256);
    counter.reallocated(256, 16);
    let stats = counter.stats();
    assert_eq!(stats.live_bytes, 16);
    assert_eq!(stats.peak_bytes, 256);
    assert_eq!(stats.allocations, 3);
}

#[test]
fn realloc_to_the_same_size_keeps_live_bytes() {
    let counter = HeapCounter::new();
    counter.allocated(32);
    counter.reallocated(32, 32);
    assert_eq!(counter.stats().live_bytes, 32);
}

#[test]
fn threads_that_free_what_they_allocate_end_at_zero() {
    let counter = Arc::new(HeapCounter::new());
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                for size in 1..=1_000 {
                    counter.allocated(size);
                    counter.reallocated(size, size * 2);
                    counter.freed(size * 2);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let stats = counter.stats();
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(stats.allocations, 16_000);
    assert!(stats.peak_bytes >= 2_000);
}

#[test]
fn a_transient_underflow_reads_as_zero_not_a_huge_number() {
    let counter = HeapCounter::new();
    counter.freed(8);
    assert_eq!(counter.stats().live_bytes, 0);
}
