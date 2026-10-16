use std::time::Instant;

use super::{WATCH_MAX_DELAY, WATCH_QUIET, WatchChange, WatchEvent};

#[derive(Default)]
pub(super) struct Batch {
    changes: Vec<WatchChange>,
    overflowed: bool,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Batch {
    pub(super) fn push(&mut self, change: WatchChange, now: Instant) {
        if !self.overflowed {
            self.changes.push(change);
        }
        self.touch(now);
    }

    pub(super) fn overflow(&mut self, now: Instant) {
        self.overflowed = true;
        self.changes = Vec::new();
        self.touch(now);
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        Some((self.last? + WATCH_QUIET).min(self.first? + WATCH_MAX_DELAY))
    }

    pub(super) fn take_due(&mut self, now: Instant) -> Option<WatchEvent> {
        if self.deadline()? > now {
            return None;
        }
        self.take()
    }

    pub(super) fn take(&mut self) -> Option<WatchEvent> {
        self.first?;
        let batch = std::mem::take(self);
        Some(if batch.overflowed {
            WatchEvent::Overflow
        } else {
            WatchEvent::Changed(batch.changes)
        })
    }

    fn touch(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }
}
