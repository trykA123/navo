use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rusty_structured::Watch;
use rusty_structured::table::{Dialect, index_records};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call forwards to `System` with the caller's layout; only counters are added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, passed on unchanged.
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: `block` came from `alloc` above with this layout.
        unsafe { System.dealloc(block, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn an_unclosed_quote_never_buffers_a_file_sized_field() {
    let mut bytes = b"id,note\n1,\"".to_vec();
    bytes.resize(bytes.len() + 24 * 1024 * 1024, b'x');
    let mut read = |offset: u64, len: usize| {
        let start = usize::try_from(offset).unwrap().min(bytes.len());
        Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
    };
    let cancel = AtomicBool::new(false);
    let mut progress = |_: u64| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let dialect = Dialect {
        delimiter: b',',
        has_header: true,
    };
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let index = index_records(&mut read, bytes.len() as u64, dialect, &mut watch)
        .unwrap()
        .unwrap();
    let grew = PEAK.load(Ordering::Relaxed) - before;
    assert_eq!(index.records, 2);
    assert!(grew < 4 * 1024 * 1024, "the index buffered {grew} bytes");
}
