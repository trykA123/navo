use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use navo_core::paging::{ByteSource, TextLayout};

use crate::model::{Budget, CancelToken, DiffStep, Reason};

use super::diff::Hunk;
#[cfg(test)]
use super::diff::diff_lines;
use super::normalise::Options;
#[cfg(test)]
use super::scan;

mod batch;
mod budget;
mod plan;
mod read;
mod schedule;

pub use budget::{Evictions, Paged, PagedState, Rediff, StepReason};
use budget::{Ledger, Store};
pub(crate) use plan::{Forge, Tuning};

pub const WORKERS: usize = 4;
pub const PROVISIONAL_BYTES: u64 = 4 * 1024 * 1024;
pub const BATCH_LINES: u64 = 16 * 1024;
pub const MAX_RESTARTS: u32 = 4;
const PROVISIONAL_LINES: u64 = 32 * 1024;
const AHEAD: usize = 4 * WORKERS;
const EVENT_BUFFER: usize = 64;
const FULL_WAIT: Duration = Duration::from_millis(5);

pub type Source = Arc<dyn ByteSource + Send + Sync>;

#[derive(Clone)]
pub struct LargeSide {
    pub source: Source,
    pub layout: TextLayout,
}

pub struct Request {
    pub left: LargeSide,
    pub right: LargeSide,
    pub options: Options,
    /// Left line in view; its batch is diffed and sent first.
    pub focus: u64,
    pub budget: Budget,
}

/// Hunks for one batch of sections; batches tile both sides once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Covered {
    pub left: Range<u64>,
    pub right: Range<u64>,
    pub hunks: Vec<Hunk>,
    /// Left line ranges of hunks emitted as coarse or paged blocks.
    pub coarse: Vec<Range<usize>>,
    /// Sections kept as counts; `state` says whether `hunks_for` can re-diff.
    pub paged: Vec<Paged>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Scan,
    Sections,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub left_lines: u64,
    pub right_lines: u64,
    pub sections: usize,
    pub dropped_anchors: usize,
    pub restarts: u32,
    pub extra_workers: u32,
}

/// After `Restarted`, every earlier `Provisional` and `Hunks` is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEvent {
    Provisional(Vec<Hunk>),
    Hunks(Covered),
    Progress(Progress),
    Step {
        step: DiffStep,
        section: Range<u64>,
        lines: u64,
        reason: StepReason,
    },
    /// An emitted section drops its line hunks and keeps this count.
    Paged {
        paged: Paged,
        seq: u64,
    },
    Done(Result<Summary, Reason>),
    Restarted,
}

type Settled = Arc<OnceLock<Result<Summary, Reason>>>;

/// Dropping it cancels the token it was started with.
pub struct LargeDiff {
    events: Receiver<DiffEvent>,
    focus: Arc<AtomicU64>,
    focus_gate: Arc<Mutex<()>>,
    cancel: CancelToken,
    outcome: Settled,
    store: Shared,
    ledger: Arc<Ledger>,
    done: Arc<AtomicBool>,
}

type Shared = Arc<Mutex<Option<Store>>>;

impl LargeDiff {
    #[must_use]
    pub fn start(request: Request, cancel: &CancelToken) -> Self {
        Self::start_tuned(request, cancel, Tuning::default())
    }

    pub(crate) fn start_tuned(request: Request, cancel: &CancelToken, tuning: Tuning) -> Self {
        let (events, receive) = mpsc::sync_channel(EVENT_BUFFER);
        let focus = Arc::new(AtomicU64::new(request.focus));
        let outcome = Settled::default();
        let store = Shared::default();
        let ledger = Arc::new(Ledger::new(request.budget));
        let done = Arc::new(AtomicBool::new(false));
        let focus_gate = Arc::new(Mutex::new(()));
        let seq = Arc::new(AtomicU64::new(0));
        let fallback = events.clone();
        let driver = Driver {
            left: request.left,
            right: request.right,
            options: request.options,
            cancel: cancel.clone(),
            events,
            focus: Arc::clone(&focus),
            focus_gate: Arc::clone(&focus_gate),
            seq: Arc::clone(&seq),
            outcome: Arc::clone(&outcome),
            tuning,
            store: Arc::clone(&store),
            ledger: Arc::clone(&ledger),
            extras: Arc::default(),
            lingering: Mutex::default(),
            done: Arc::clone(&done),
        };
        let spawned = thread::Builder::new()
            .name("large-diff".to_owned())
            .spawn(move || driver.run());
        if spawned.is_err() {
            let _first = outcome.set(Err(Reason::WorkerPanicked));
            let failed = fallback
                .try_send(DiffEvent::Done(Err(Reason::WorkerPanicked)))
                .is_err();
            debug_assert!(!failed, "the receiver is local until start returns");
        }
        Self {
            events: receive,
            focus,
            focus_gate,
            cancel: cancel.clone(),
            outcome,
            store,
            ledger,
            done,
        }
    }

    #[must_use]
    pub fn events(&self) -> &Receiver<DiffEvent> {
        &self.events
    }

    /// The final result, kept even when the `Done` event could not be queued.
    #[must_use]
    pub fn outcome(&self) -> Option<Result<Summary, Reason>> {
        self.outcome.get().cloned()
    }

    /// The caller must apply the returned evictions, then acknowledge them.
    pub fn set_focus(&self, line: u64) -> Evictions {
        self.focus.store(line, Ordering::Relaxed);
        let _gate = self
            .focus_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !self.done.load(Ordering::Acquire) {
            return Evictions::empty(self.ledger.empty());
        }
        let mut store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
        match store.as_mut() {
            Some(store) => store.page(line),
            None => Evictions::empty(self.ledger.empty()),
        }
    }
}

impl Drop for LargeDiff {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct Driver {
    left: LargeSide,
    right: LargeSide,
    options: Options,
    cancel: CancelToken,
    events: SyncSender<DiffEvent>,
    focus: Arc<AtomicU64>,
    focus_gate: Arc<Mutex<()>>,
    seq: Arc<AtomicU64>,
    outcome: Settled,
    tuning: Tuning,
    store: Shared,
    ledger: Arc<Ledger>,
    extras: Arc<AtomicU32>,
    lingering: Mutex<Vec<JoinHandle<()>>>,
    done: Arc<AtomicBool>,
}

impl Driver {
    fn run(self) {
        let mut restarts = 0;
        loop {
            let attempt = catch_unwind(AssertUnwindSafe(|| self.attempt()))
                .unwrap_or(Err(Reason::WorkerPanicked));
            let restart = attempt == Err(Reason::ChangedOnDisk)
                && restarts < MAX_RESTARTS
                && !self.cancel.is_cancelled();
            if !restart {
                let settled = self.settle(attempt, restarts);
                let _first = self.outcome.set(settled.clone());
                self.before_done();
                self.finish(settled);
                return;
            }
            restarts += 1;
            if !self.post(DiffEvent::Restarted) {
                let _first = self.outcome.set(Err(Reason::Stopped));
                self.done.store(true, Ordering::Release);
                self.reap(false);
                return;
            }
        }
    }

    fn before_done(&self) {
        if let Some(before) = &self.tuning.before_done {
            before(&self.focus);
        }
    }

    fn finish(&self, settled: Result<Summary, Reason>) {
        let gate = self
            .focus_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let focus = self.focus.load(Ordering::Relaxed);
        let evictions = self.page(focus);
        let (paged, first) = self.defer(evictions);
        self.done.store(true, Ordering::Release);
        drop(gate);
        let _sent = schedule::paged::send(self, &paged, first);
        let _done = self.post(DiffEvent::Done(settled));
        self.reap(false);
    }

    fn page(&self, focus: u64) -> Evictions {
        let mut store = self.store_slot();
        match store.as_mut() {
            Some(store) => store.page(focus),
            None => Evictions::empty(self.ledger.empty()),
        }
    }

    fn defer(&self, evictions: Evictions) -> (Vec<Paged>, u64) {
        let mut store = self.store_slot();
        match store.as_mut() {
            Some(store) => store.defer(evictions),
            None => {
                let (paged, hold) = evictions.into_parts();
                drop(hold);
                (paged, 0)
            }
        }
    }

    fn settle(&self, attempt: Result<Summary, Reason>, restarts: u32) -> Result<Summary, Reason> {
        match attempt {
            Ok(summary) => Ok(Summary {
                restarts,
                ..summary
            }),
            Err(_) if self.cancel.is_cancelled() => Err(Reason::Stopped),
            Err(reason) => Err(reason),
        }
    }

    fn attempt(&self) -> Result<Summary, Reason> {
        self.reap(false);
        self.store_slot().take();
        for side in [&self.left, &self.right] {
            let _stale = side.source.take_changed() | side.source.take_rewritten();
        }
        let (left, right) = self.scan_phase()?;
        let plan = self.plan(left, right);
        let mut schedule = plan.schedule;
        self.sections_phase(&plan.context, &mut schedule, plan.limits)?;
        for side in [&plan.context.left, &plan.context.right] {
            side.source.probe_end().map_err(|_| Reason::ReadError)?;
            batch::unchanged(side)?;
        }
        Ok(Summary {
            left_lines: plan.context.left.lines,
            right_lines: plan.context.right.lines,
            sections: schedule.counts.sections,
            dropped_anchors: schedule.counts.dropped,
            restarts: 0,
            extra_workers: self.extras.load(Ordering::Acquire),
        })
    }

    fn store_slot(&self) -> std::sync::MutexGuard<'_, Option<Store>> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn linger(&self, handles: Vec<JoinHandle<()>>) {
        let mut lingering = self
            .lingering
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        lingering.extend(handles);
    }

    /// Joins finished section workers; with `all`, waits for late ones too.
    fn reap(&self, all: bool) {
        let mut lingering = self
            .lingering
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (done, late): (Vec<_>, Vec<_>) = std::mem::take(&mut *lingering)
            .into_iter()
            .partition(|handle| all || handle.is_finished());
        *lingering = late;
        drop(lingering);
        for handle in done {
            let _joined = handle.join();
        }
    }

    fn hint(&self, stage: Stage, done: u64, total: u64) {
        let progress = Progress { stage, done, total };
        let _dropped = self.events.try_send(DiffEvent::Progress(progress));
    }

    fn send(&self, event: DiffEvent) -> Result<(), Reason> {
        if self.post(event) {
            Ok(())
        } else {
            Err(Reason::Stopped)
        }
    }

    fn post(&self, event: DiffEvent) -> bool {
        let mut pending = event;
        loop {
            pending = match self.events.try_send(pending) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(returned)) => returned,
            };
            if self.cancel.is_cancelled() {
                return false;
            }
            thread::sleep(FULL_WAIT);
        }
    }
}

#[cfg(test)]
mod tests;
