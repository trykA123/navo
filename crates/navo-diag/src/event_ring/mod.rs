use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RingStats {
    pub events: usize,
    pub bytes: usize,
    pub dropped: u64,
    pub rejected: u64,
}

#[derive(Default)]
struct Events {
    queue: VecDeque<Arc<str>>,
    bytes: usize,
}

pub struct EventRing {
    events: Mutex<Events>,
    budget: usize,
    dropped: AtomicU64,
    rejected: AtomicU64,
}

impl EventRing {
    pub const DEFAULT_BUDGET: usize = 2 * 1024 * 1024;

    #[must_use]
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            events: Mutex::new(Events::default()),
            budget: budget_bytes,
            dropped: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn push(&self, event: &str) {
        if event.len() > self.budget {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let event: Arc<str> = Arc::from(event);
        let mut events = self.lock();
        events.bytes += event.len();
        events.queue.push_back(event);
        let mut dropped = 0;
        while events.bytes > self.budget {
            let Some(old) = events.queue.pop_front() else {
                break;
            };
            events.bytes -= old.len();
            dropped += 1;
        }
        drop(events);
        if dropped > 0 {
            self.dropped.fetch_add(dropped, Ordering::Relaxed);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<Arc<str>> {
        self.lock().queue.iter().cloned().collect()
    }

    #[must_use]
    pub fn stats(&self) -> RingStats {
        let events = self.lock();
        RingStats {
            events: events.queue.len(),
            bytes: events.bytes,
            dropped: self.dropped.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
        }
    }

    pub fn clear(&self) {
        let mut events = self.lock();
        events.queue.clear();
        events.bytes = 0;
    }

    fn lock(&self) -> MutexGuard<'_, Events> {
        self.events.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;
