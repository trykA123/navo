use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// App-wide cap on Git child processes; the scanner and the fetch timer share one.
#[derive(Debug, Clone)]
pub(crate) struct Slots {
    used: Arc<AtomicUsize>,
    cap: usize,
}

/// One running Git process; the slot frees on drop.
#[derive(Debug)]
pub(crate) struct Slot(Arc<AtomicUsize>);

impl Slots {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            used: Arc::new(AtomicUsize::new(0)),
            cap,
        }
    }

    pub(crate) fn try_acquire(&self) -> Option<Slot> {
        self.used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < self.cap).then_some(used + 1)
            })
            .ok()
            .map(|_| Slot(self.used.clone()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
