use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub(super) fn flag(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
