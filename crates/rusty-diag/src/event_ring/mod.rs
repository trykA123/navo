use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RingStats {
    pub events: usize,
    pub bytes: usize,
    pub dropped: u64,
    pub rejected: u64,
}

pub struct EventRing {
    events: Mutex<VecDeque<Arc<str>>>,
    budget: usize,
    dropped: AtomicU64,
    rejected: AtomicU64,
}

impl EventRing {
    pub const DEFAULT_BUDGET: usize = 2 * 1024 * 1024;

    pub fn new(budget_bytes: usize) -> Self {
        todo!("{budget_bytes}")
    }

    pub fn push(&self, event: &str) {
        todo!("{event}")
    }

    pub fn snapshot(&self) -> Vec<Arc<str>> {
        todo!()
    }

    pub fn stats(&self) -> RingStats {
        todo!()
    }

    pub fn clear(&self) {
        todo!()
    }
}

#[cfg(test)]
mod tests;
