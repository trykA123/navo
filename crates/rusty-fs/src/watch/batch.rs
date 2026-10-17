use std::collections::HashMap;
use std::time::Instant;

use rusty_core::ProviderPath;

use super::{WATCH_MAX_DELAY, WATCH_QUIET, WatchChange, WatchEvent};

#[derive(Default)]
pub(super) struct Batch {
    changes: Vec<WatchChange>,
    latest: HashMap<ProviderPath, usize>,
    overflowed: bool,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Batch {
    pub(super) fn for_tree() -> Self {
        todo!()
    }

    pub(super) fn push(&mut self, change: WatchChange, now: Instant) {
        self.touch(now);
        if self.overflowed {
            return;
        }
        let Some(&at) = self.latest.get(name_of(&change)) else {
            return self.append(change);
        };
        let earlier = &self.changes[at];
        let is_write = matches!(change, WatchChange::Modified(_));
        if is_write
            && matches!(
                earlier,
                WatchChange::Created(_) | WatchChange::Renamed { .. }
            )
        {
            return;
        }
        if matches!(earlier, WatchChange::Renamed { .. }) {
            return self.append(change);
        }
        self.changes[at] = change;
    }

    fn append(&mut self, change: WatchChange) {
        self.latest
            .insert(name_of(&change).clone(), self.changes.len());
        self.changes.push(change);
    }

    pub(super) fn overflow(&mut self, now: Instant) {
        self.overflowed = true;
        self.changes = Vec::new();
        self.latest = HashMap::new();
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

fn name_of(change: &WatchChange) -> &ProviderPath {
    match change {
        WatchChange::Created(name) | WatchChange::Removed(name) | WatchChange::Modified(name) => {
            name
        }
        WatchChange::Renamed { to, .. } => to,
    }
}
