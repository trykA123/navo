use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, PoisonError};

use crate::model::Reason;

use super::batch::{Context, Outcome};
use super::budget::{
    Charge, Evictions, Limits, MAX_EXTRA_WORKERS, Out, Paged, PagedState, Part, Recorded,
    WATCH_POLL,
};
use super::{DiffEvent, Driver, Progress, Stage, WORKERS};
use cover::cover;
use paged::step_event;
use pool::{Common, Pool, Reply, StopFn};
use slots::State;

mod cover;
mod group;
pub(super) mod paged;
mod pool;
mod slots;

pub(super) use group::group;
pub(super) use pool::{Beat, Finished};
pub(crate) use slots::Schedule;

impl Driver {
    pub(super) fn sections_phase(
        &self,
        context: &Arc<Context>,
        schedule: &mut Schedule,
        limits: Arc<Limits>,
    ) -> Result<(), Reason> {
        let halt = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let signal = Arc::clone(&halt);
        let stop: StopFn =
            Arc::new(move || cancel.is_cancelled() || signal.load(Ordering::Acquire));
        let common = Common {
            context: Arc::clone(context),
            limits: Arc::clone(&limits),
            halt: stop,
            finished: self.tuning.finished.clone(),
        };
        let _held = if self.tuning.hold_all {
            limits.ledger.reserve(limits.ledger.available())
        } else if self.tuning.hold_available {
            let room = limits
                .ledger
                .available()
                .saturating_sub(Out::block_bytes().saturating_add(1024));
            limits.ledger.reserve(room)
        } else {
            None
        };
        let workers = WORKERS + MAX_EXTRA_WORKERS as usize;
        let (replies, receiver) = mpsc::sync_channel::<Reply>(workers);
        let mut pool = Pool::new(common, replies, Arc::clone(&self.extras));
        let started = (0..WORKERS.min(schedule.slots.len())).try_for_each(|_| pool.spawn());
        let result = started.and_then(|()| {
            let mut run = Run {
                driver: self,
                state: schedule,
                replies: &receiver,
                pool: &mut pool,
                limits: &limits,
            };
            run.run()
        });
        halt.store(true, Ordering::Release);
        drop(receiver);
        self.linger(pool.close());
        result
    }
}

struct Run<'a> {
    driver: &'a Driver,
    state: &'a mut Schedule,
    replies: &'a Receiver<Reply>,
    pool: &'a mut Pool,
    limits: &'a Limits,
}

impl Run<'_> {
    fn run(&mut self) -> Result<(), Reason> {
        loop {
            if self.driver.cancel.is_cancelled() {
                return Err(Reason::Stopped);
            }
            if self.limits.ledger.take_wake() {
                self.state.wake();
            }
            self.follow_focus()?;
            self.flush()?;
            self.fill()?;
            self.watch()?;
            self.recover()?;
            if self.state.live() == 0 && self.state.next_emit == self.state.slots.len() {
                return Ok(());
            }
            if self.state.live() == 0 && self.state.flights.is_empty() && !self.state.has_pending()
            {
                return Err(Reason::WorkerPanicked);
            }
            match self.replies.recv_timeout(WATCH_POLL) {
                Ok(reply) => self.handle(reply)?,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(Reason::WorkerPanicked),
            }
            self.pool.reap();
        }
    }

    fn fill(&mut self) -> Result<(), Reason> {
        while self.state.live() < WORKERS {
            if self.driver.cancel.is_cancelled() {
                return Err(Reason::Stopped);
            }
            let Some(worker) = self.pool.idle() else {
                break;
            };
            let focus = self.driver.focus.load(Ordering::Relaxed);
            let Some(index) = self.state.pick(focus) else {
                break;
            };
            let job = self.state.dispatch(index, worker);
            let id = job.id;
            if !self.pool.dispatch(worker, job)? {
                self.state.undo(id);
                break;
            }
        }
        Ok(())
    }

    fn watch(&mut self) -> Result<(), Reason> {
        let limit = self.driver.tuning.watchdog;
        let overdue: Vec<_> = self
            .state
            .flights
            .iter()
            .filter(|flight| flight.live)
            .filter_map(|flight| {
                let pair = self.pool.overdue(flight.worker, flight.job, limit)?;
                Some((flight.worker, flight.job, pair))
            })
            .collect();
        for (worker, id, pair) in overdue {
            self.state.expire(id, pair);
            self.pool.abandon(worker, id);
            if let Some(expired) = self.pool.expired() {
                expired(id);
            }
            let _spawned = self.pool.spawn_extra()?;
        }
        Ok(())
    }

    fn recover(&mut self) -> Result<(), Reason> {
        if self.state.live() != 0 || self.pool.idle().is_some() || !self.state.has_pending() {
            return Ok(());
        }
        while self.pool.recovering() < WORKERS {
            let Some(index) = self.state.pending() else {
                break;
            };
            let job = self.state.dispatch_recovery(index);
            self.pool.recover_job(job)?;
        }
        self.state.clear_waits();
        Ok(())
    }

    fn handle(&mut self, reply: Reply) -> Result<(), Reason> {
        self.pool.free(reply.worker, reply.id);
        let Some(flight) = self.state.take_flight(reply.id) else {
            return Ok(());
        };
        let Some(index) = self.state.slot_of(&flight).filter(|_| flight.live) else {
            return Ok(());
        };
        let focused = self.state.focus_job == Some(reply.id);
        if focused {
            self.state.focus_job = None;
        }
        match reply.outcome? {
            Outcome::Covered { out, counts } => {
                self.state.slots[index].state = State::Ready(out, counts);
                if focused {
                    self.emit(index)?;
                }
            }
            Outcome::Collision { start, end } => self.collide(index, (start, end), focused)?,
            Outcome::Retry => {
                self.state.retry(index);
                let progress = Progress {
                    stage: Stage::Waiting,
                    done: self.state.emitted,
                    total: self.state.total,
                };
                self.driver.send(DiffEvent::Progress(progress))?;
                if focused {
                    self.state.refocus(index);
                }
            }
        }
        Ok(())
    }

    fn collide(
        &mut self,
        index: usize,
        (start, end): (bool, bool),
        focused: bool,
    ) -> Result<(), Reason> {
        self.state.slots[index].state = State::Pending;
        let mut given_up = Vec::new();
        if end && let Some(given) = self.merge_or_drop(index)? {
            given_up.extend(given);
        }
        let merged = if start {
            index.checked_sub(1).ok_or(Reason::ChangedOnDisk)?
        } else {
            index
        };
        if start && let Some(given) = self.merge_or_drop(merged)? {
            given_up.extend(given);
        }
        for (worker, id) in given_up {
            self.pool.abandon(worker, id);
        }
        if focused {
            self.state.refocus(merged);
        }
        Ok(())
    }

    fn merge_or_drop(&mut self, index: usize) -> Result<Option<Vec<(usize, u64)>>, Reason> {
        match self.state.merge(index) {
            Ok(given_up) => Ok(Some(given_up)),
            Err(Reason::Unsupported) => {
                self.state.slots[index].batch.end_anchor = false;
                self.state.slots[index].batch.terminal = false;
                if let Some(next) = self.state.slots.get_mut(index + 1) {
                    next.batch.start_anchor = false;
                }
                Ok(None)
            }
            Err(reason) => Err(reason),
        }
    }

    fn flush(&mut self) -> Result<(), Reason> {
        if self.state.waits_for_focus() {
            return Ok(());
        }
        while let Some(slot) = self.state.slots.get(self.state.next_emit) {
            match slot.state {
                State::Ready(..) => self.emit(self.state.next_emit)?,
                State::Emitted => {}
                State::Pending | State::Running(_) => return Ok(()),
            }
            self.state.next_emit += 1;
        }
        Ok(())
    }

    fn emit(&mut self, index: usize) -> Result<(), Reason> {
        let slot = &mut self.state.slots[index];
        let State::Ready(out, counts) = std::mem::replace(&mut slot.state, State::Emitted) else {
            return Ok(());
        };
        let (left, right, lines) = (slot.batch.left(), slot.batch.right(), slot.batch.lines());
        self.state.counts.sections += counts.sections;
        self.state.counts.dropped += counts.dropped;
        self.state.emitted = self.state.emitted.saturating_add(lines);
        let focus = self.driver.focus.load(Ordering::Relaxed);
        let (mut covered, steps, parts, charge) = cover(out, left, right);
        let recorded = self.record(
            &parts,
            focus,
            (covered.hunks.capacity(), covered.digests.capacity(), charge),
        );
        if matches!(&recorded, Recorded::CountsOnly) {
            for paged in &mut covered.paged {
                paged.state = PagedState::CountsOnly;
            }
        }
        for step in &steps {
            self.driver.send(step_event(step))?;
        }
        self.driver.send(DiffEvent::Hunks(covered))?;
        match recorded {
            Recorded::Evictions(evictions) => {
                let (paged, first) = self.defer(evictions);
                paged::send(self.driver, &paged, first)?;
            }
            Recorded::CountsOnly => self.send_counts(&parts)?,
        }
        self.driver
            .hint(Stage::Sections, self.state.emitted, self.state.total);
        Ok(())
    }

    fn follow_focus(&mut self) -> Result<(), Reason> {
        let focus = self.driver.focus.load(Ordering::Relaxed);
        let evictions = self.store(|store| Ok(store.page(focus)))?;
        let (paged, first) = self.defer(evictions);
        paged::send(self.driver, &paged, first)?;
        Ok(())
    }

    fn send_counts(&self, parts: &[Part]) -> Result<(), Reason> {
        paged::send_counts(self.driver, parts)
    }

    fn record(&self, parts: &[Part], focus: u64, allocation: (usize, usize, Charge)) -> Recorded {
        let mut store = self
            .driver
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match store.as_mut() {
            Some(store) => store.record(parts, focus, allocation),
            None => Recorded::CountsOnly,
        }
    }

    fn store(
        &self,
        act: impl FnOnce(&mut super::budget::Store) -> Result<Evictions, Reason>,
    ) -> Result<Evictions, Reason> {
        let mut store = self
            .driver
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match store.as_mut() {
            Some(store) => act(store),
            None => Ok(Evictions::empty(self.driver.ledger.empty())),
        }
    }

    fn defer(&self, evictions: Evictions) -> (Vec<Paged>, u64) {
        let mut store = self
            .driver
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match store.as_mut() {
            Some(store) => store.defer(evictions),
            None => {
                let (paged, hold) = evictions.into_parts();
                drop(hold);
                (paged, 0)
            }
        }
    }
}

#[cfg(test)]
mod tests;
