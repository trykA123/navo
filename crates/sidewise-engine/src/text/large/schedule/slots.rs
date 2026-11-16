use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::anchor::SectionPair;
use crate::model::Reason;

use super::super::AHEAD;
use super::super::batch::{Batch, Counts, Sections};
use super::super::budget::{Out, bytes_of};
use super::pool::Job;

pub(super) enum State {
    Pending,
    Running(u64),
    Ready(Out, Counts),
    Emitted,
}

const RETRY_START: Duration = Duration::from_millis(10);
const RETRY_MAX: Duration = Duration::from_secs(1);

pub(super) struct Retry {
    attempt: u32,
    ready: Instant,
}

impl Retry {
    fn ready(&self) -> bool {
        self.ready <= Instant::now()
    }
}

pub(super) struct Slot {
    key: u64,
    pub(super) batch: Batch,
    forced: Vec<SectionPair>,
    pub(super) state: State,
    retry: Option<Retry>,
}

fn retry_delay(attempt: u32) -> Duration {
    RETRY_START
        .saturating_mul(1_u32.checked_shl(attempt.min(7)).unwrap_or(128))
        .min(RETRY_MAX)
}

/// A dispatched job; `live` is false once the watchdog or a merge gave it up.
pub(super) struct Flight {
    pub(super) job: u64,
    pub(super) worker: usize,
    slot: u64,
    pub(super) live: bool,
}

/// Batches in line order; jobs, focus and merges are tracked by stable ids.
pub(crate) struct Schedule {
    pub(super) slots: Vec<Slot>,
    pub(super) flights: Vec<Flight>,
    pub(super) next_emit: usize,
    pub(super) focus_job: Option<u64>,
    refocus: Option<u64>,
    next_id: u64,
    pub(crate) counts: Counts,
    pub(super) emitted: u64,
    pub(super) total: u64,
}

impl Slot {
    fn ready(&self) -> bool {
        matches!(self.state, State::Pending) && self.retry.as_ref().is_none_or(Retry::ready)
    }
}

impl Schedule {
    pub(crate) fn new(batches: Vec<Batch>) -> Self {
        let total = batches.iter().map(Batch::lines).sum();
        let slots = batches
            .into_iter()
            .zip(0..)
            .map(|(batch, key)| Slot {
                key,
                batch,
                forced: Vec::new(),
                state: State::Pending,
                retry: None,
            })
            .collect();
        Self {
            slots,
            flights: Vec::new(),
            next_emit: 0,
            focus_job: None,
            refocus: None,
            next_id: 0,
            counts: Counts::default(),
            emitted: 0,
            total,
        }
    }

    /// Bytes of the batch slots themselves.
    pub(crate) fn bytes(&self) -> u64 {
        bytes_of::<Slot>(self.slots.capacity())
    }

    pub(super) fn waits_for_focus(&self) -> bool {
        self.focus_job.is_some() || self.refocus.is_some()
    }

    pub(super) fn pick(&mut self, focus: u64) -> Option<usize> {
        if let Some(key) = self.refocus.take()
            && let Some(index) = self.index_of(key)
            && self.slots[index].ready()
        {
            self.focus_job = Some(self.next_id);
            return Some(index);
        }
        let last = self.slots.len().checked_sub(1)?;
        let focused = self
            .slots
            .partition_point(|slot| slot.batch.left().end <= focus)
            .min(last);
        if self.focus_job.is_none() && self.slots[focused].ready() {
            self.focus_job = Some(self.next_id);
            return Some(focused);
        }
        let end = self.slots.len().min(self.next_emit + AHEAD);
        (self.next_emit..end).find(|&index| self.slots[index].ready())
    }

    pub(super) fn pending(&self) -> Option<usize> {
        self.slots.iter().position(Slot::ready)
    }

    pub(super) fn dispatch(&mut self, index: usize, worker: usize) -> Job {
        self.dispatch_at(index, worker)
    }

    pub(super) fn dispatch_recovery(&mut self, index: usize) -> Job {
        self.dispatch_at(index, usize::MAX)
    }

    pub(super) fn retry(&mut self, index: usize) {
        let slot = &mut self.slots[index];
        let attempt = slot
            .retry
            .as_ref()
            .map_or(1, |retry| retry.attempt.saturating_add(1));
        slot.state = State::Pending;
        slot.retry = Some(Retry {
            attempt,
            ready: Instant::now() + retry_delay(attempt.saturating_sub(1)),
        });
    }

    pub(super) fn wake(&mut self) {
        for slot in &mut self.slots {
            if let Some(retry) = &mut slot.retry {
                retry.ready = Instant::now();
            }
        }
    }

    fn dispatch_at(&mut self, index: usize, worker: usize) -> Job {
        let id = self.next_id;
        self.next_id += 1;
        let slot = &mut self.slots[index];
        slot.state = State::Running(id);
        self.flights.push(Flight {
            job: id,
            worker,
            slot: slot.key,
            live: true,
        });
        Job {
            id,
            batch: slot.batch.clone(),
            forced: slot.forced.clone(),
        }
    }

    /// Puts back a job no worker took.
    pub(super) fn undo(&mut self, id: u64) {
        if let Some(flight) = self.take_flight(id)
            && let Some(index) = self.index_of(flight.slot)
        {
            self.slots[index].state = State::Pending;
            self.release_focus(id, flight.slot);
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| matches!(slot.state, State::Pending))
    }

    pub(super) fn clear_waits(&mut self) {
        self.focus_job = None;
        self.refocus = None;
    }

    pub(super) fn live(&self) -> usize {
        self.flights.iter().filter(|flight| flight.live).count()
    }

    pub(super) fn take_flight(&mut self, id: u64) -> Option<Flight> {
        let at = self.flights.iter().position(|flight| flight.job == id)?;
        Some(self.flights.swap_remove(at))
    }

    pub(super) fn slot_of(&self, flight: &Flight) -> Option<usize> {
        self.index_of(flight.slot)
    }

    fn index_of(&self, key: u64) -> Option<usize> {
        self.slots.iter().position(|slot| slot.key == key)
    }

    fn give_up(&mut self, id: u64, key: u64, given_up: &mut Vec<(usize, u64)>) {
        if let Some(flight) = self.flights.iter_mut().find(|flight| flight.job == id) {
            flight.live = false;
            given_up.push((flight.worker, id));
        }
        self.release_focus(id, key);
    }

    fn release_focus(&mut self, id: u64, key: u64) {
        if self.focus_job == Some(id) {
            self.focus_job = None;
            self.refocus = Some(key);
        }
    }

    pub(super) fn refocus(&mut self, index: usize) {
        self.refocus = Some(self.slots[index].key);
    }

    /// Gives up a job whose section passed the deadline; the batch runs again with it coarse.
    pub(super) fn expire(&mut self, id: u64, pair: SectionPair) {
        let Some(flight) = self.flights.iter_mut().find(|flight| flight.job == id) else {
            return;
        };
        flight.live = false;
        let key = flight.slot;
        if let Some(index) = self.index_of(key) {
            let slot = &mut self.slots[index];
            slot.forced.push(pair);
            slot.state = State::Pending;
            slot.retry = None;
        }
        self.release_focus(id, key);
    }

    /// Joins slot `index + 1` into `index`; returns the workers and jobs given up on the way.
    pub(super) fn merge(&mut self, index: usize) -> Result<Vec<(usize, u64)>, Reason> {
        let emitted = |slot: &Slot| matches!(slot.state, State::Emitted);
        if index + 1 >= self.slots.len() || self.slots[index..=index + 1].iter().any(emitted) {
            return Err(Reason::ChangedOnDisk);
        }
        let first = &self.slots[index];
        let next = &self.slots[index + 1];
        let room = first.batch.sections.list.len() + next.batch.sections.list.len();
        let mut charge = first.batch.sections.charge.fresh();
        if !charge.try_add(bytes_of::<SectionPair>(room)) {
            return Err(Reason::Unsupported);
        }
        let key = self.slots[index].key;
        let mut given_up = Vec::new();
        for at in [index, index + 1] {
            if let State::Running(id) = self.slots[at].state {
                self.give_up(id, key, &mut given_up);
            }
        }
        let next = self.slots.remove(index + 1);
        let slot = &mut self.slots[index];
        let mut list = Vec::with_capacity(room);
        list.extend_from_slice(&slot.batch.sections.list);
        list.extend_from_slice(&next.batch.sections.list);
        charge.settle(bytes_of::<SectionPair>(list.capacity()));
        slot.batch.sections = Arc::new(Sections { list, charge });
        slot.batch.end_anchor = next.batch.end_anchor;
        slot.batch.terminal = next.batch.terminal;
        slot.forced.extend(next.forced);
        slot.state = State::Pending;
        slot.retry = None;
        Ok(given_up)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::anchor::SectionPair;
    use crate::model::{Budget, Reason};

    use super::super::super::batch::{Batch, Sections};
    use super::super::super::budget::Ledger;
    use super::Schedule;

    fn batch(ledger: &Arc<Ledger>) -> Batch {
        Batch {
            sections: Arc::new(Sections {
                list: vec![SectionPair {
                    left: 0..1,
                    right: 0..1,
                }],
                charge: ledger.empty(),
            }),
            start_anchor: false,
            end_anchor: false,
            terminal: false,
        }
    }

    #[test]
    fn a_merge_that_cannot_reserve_does_not_mutate_or_allocate() {
        let ledger = Arc::new(Ledger::new(Budget::from_bytes(1)));
        let mut schedule = Schedule::new(vec![batch(&ledger), batch(&ledger)]);
        assert!(matches!(schedule.merge(0), Err(Reason::Unsupported)));
        assert_eq!(schedule.slots.len(), 2, "neither batch was consumed");
        assert_eq!(ledger.used(), 0, "nothing was reserved");
    }
}
