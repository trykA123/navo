use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct Capacity {
    construction: AtomicU64,
    regions: AtomicU64,
}

impl Capacity {
    pub(super) fn peak(&self) -> (u64, u64) {
        (
            self.construction.load(Ordering::Relaxed),
            self.regions.load(Ordering::Relaxed),
        )
    }
}

thread_local! {
    static CAPACITY: RefCell<Option<Arc<Capacity>>> = const { RefCell::new(None) };
}

pub(super) fn install(capacity: Arc<Capacity>) {
    CAPACITY.set(Some(capacity));
}

pub(super) fn record(construction: u64, regions: u64) {
    CAPACITY.with_borrow(|capacity| {
        if let Some(capacity) = capacity {
            capacity
                .construction
                .fetch_max(construction, Ordering::Relaxed);
            capacity.regions.fetch_max(regions, Ordering::Relaxed);
        }
    });
}
