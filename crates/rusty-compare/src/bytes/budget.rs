use std::sync::mpsc::SyncSender;

use crate::anchor::Candidate;
use crate::model::{Budget, CancelToken, Reason};

use super::offset::post;
use super::{ByteEvent, ByteRegion};

pub(super) const CHUNK_BYTES: u64 = std::mem::size_of::<Candidate<[u8; 32]>>() as u64;
pub(super) const REGION_BYTES: u64 = std::mem::size_of::<ByteRegion>() as u64;
const PROGRESS_EVERY: u64 = 1024 * 1024;

pub(super) struct Regions {
    limit: u64,
    entries: Vec<ByteRegion>,
    next: usize,
}

impl Regions {
    pub(super) fn new(budget: Budget) -> Self {
        Self {
            limit: budget.bytes() / 4,
            entries: Vec::new(),
            next: 0,
        }
    }

    pub(super) fn push(&mut self, region: ByteRegion) {
        if self.entries.len() == self.entries.capacity() {
            let available = usize::try_from(self.limit / REGION_BYTES).unwrap_or(usize::MAX);
            let capacity = self
                .entries
                .capacity()
                .max(2)
                .saturating_mul(2)
                .min(available);
            if capacity > self.entries.capacity() {
                self.entries.reserve_exact(capacity - self.entries.len());
            } else if let Some(pending) = self.entries.last_mut() {
                merge(pending, region);
                return;
            }
        }
        self.entries.push(region);
        #[cfg(test)]
        super::retained::record(0, self.used());
    }

    pub(super) fn finish(&mut self) -> Option<ByteRegion> {
        let region = self.entries.get(self.next)?.clone();
        self.next += 1;
        Some(region)
    }

    #[cfg(test)]
    pub(super) fn used(&self) -> u64 {
        u64::try_from(self.entries.capacity())
            .unwrap_or(u64::MAX)
            .saturating_mul(REGION_BYTES)
    }

    #[cfg(test)]
    pub(super) fn limit(&self) -> u64 {
        self.limit
    }
}

fn merge(into: &mut ByteRegion, next: ByteRegion) {
    into.left.start = into.left.start.min(next.left.start);
    into.left.end = into.left.end.max(next.left.end);
    into.right.start = into.right.start.min(next.right.start);
    into.right.end = into.right.end.max(next.right.end);
    into.regions = into.regions.saturating_add(next.regions);
    into.changed = into.changed.saturating_add(next.changed);
}

#[cfg(test)]
#[path = "tests/budget.rs"]
mod tests;

pub(super) struct Reporter<'a> {
    events: &'a SyncSender<ByteEvent>,
    pub(super) cancel: &'a CancelToken,
    total: u64,
    done: u64,
    next: u64,
    regions: Regions,
    closed: bool,
}

impl<'a> Reporter<'a> {
    pub(super) fn new(
        events: &'a SyncSender<ByteEvent>,
        cancel: &'a CancelToken,
        total: u64,
        budget: Budget,
    ) -> Self {
        Self {
            events,
            cancel,
            total,
            done: 0,
            next: PROGRESS_EVERY,
            regions: Regions::new(budget),
            closed: false,
        }
    }

    pub(super) fn cancelled(&self) -> bool {
        self.closed || self.cancel.is_cancelled()
    }

    pub(super) fn add(&mut self, bytes: u64) {
        self.done = self.done.saturating_add(bytes);
        if self.done >= self.next && self.total > 0 {
            self.next = self.done.saturating_add(PROGRESS_EVERY);
            let done = self.done.min(self.total);
            self.send(ByteEvent::Progress {
                done,
                total: self.total,
            });
        }
    }

    pub(super) fn region(&mut self, region: ByteRegion) -> Result<(), Reason> {
        if self.cancelled() {
            return Err(Reason::Stopped);
        }
        self.regions.push(region);
        Ok(())
    }

    pub(super) fn finish(&mut self) -> bool {
        while let Some(region) = self.regions.finish() {
            if !self.send(ByteEvent::Region(region)) {
                return false;
            }
        }
        !self.closed
    }

    pub(super) fn send(&mut self, event: ByteEvent) -> bool {
        if self.closed || self.cancel.is_cancelled() {
            return false;
        }
        let sent = post(self.events, self.cancel, event);
        if !sent {
            self.closed = true;
        }
        sent
    }
}
