use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use rusty_core::paging::{ByteSource, TextLayout};

use crate::anchor;
use crate::model::{CancelToken, Reason};

use super::diff::{Hunk, diff_lines};
use super::normalise::Options;
use super::scan::{self, BUCKETS, ScanResult};

mod batch;
mod read;
mod schedule;

use batch::{Batch, Context, SideData};
use schedule::{Schedule, group};

pub const WORKERS: usize = 4;
pub const PROVISIONAL_BYTES: u64 = 4 * 1024 * 1024;
pub const BATCH_LINES: u64 = 16 * 1024;
pub const MAX_RESTARTS: u32 = 4;
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
}

/// Hunks for one batch of sections; batches tile both sides once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Covered {
    pub left: Range<u64>,
    pub right: Range<u64>,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Scan,
    Sections,
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
}

/// After `Restarted`, every earlier `Provisional` and `Hunks` is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEvent {
    Provisional(Vec<Hunk>),
    Hunks(Covered),
    Progress(Progress),
    Done(Result<Summary, Reason>),
    Restarted,
}

type Settled = Arc<OnceLock<Result<Summary, Reason>>>;

pub(crate) type Forge =
    Arc<dyn Fn(&mut Vec<scan::Candidate>, &mut Vec<scan::Candidate>) + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Tuning {
    pub(crate) batch_lines: u64,
    pub(crate) forge: Option<Forge>,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            batch_lines: BATCH_LINES,
            forge: None,
        }
    }
}

/// Dropping it cancels the token it was started with.
pub struct LargeDiff {
    events: Receiver<DiffEvent>,
    focus: Arc<AtomicU64>,
    cancel: CancelToken,
    outcome: Settled,
}

impl LargeDiff {
    #[must_use]
    pub fn start(request: Request, cancel: &CancelToken) -> Self {
        Self::start_tuned(request, cancel, Tuning::default())
    }

    pub(crate) fn start_tuned(request: Request, cancel: &CancelToken, tuning: Tuning) -> Self {
        let (events, receive) = mpsc::sync_channel(EVENT_BUFFER);
        let focus = Arc::new(AtomicU64::new(request.focus));
        let outcome = Settled::default();
        let fallback = events.clone();
        let driver = Driver {
            left: request.left,
            right: request.right,
            options: request.options,
            cancel: cancel.clone(),
            events,
            focus: Arc::clone(&focus),
            outcome: Arc::clone(&outcome),
            tuning,
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
            cancel: cancel.clone(),
            outcome,
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

    pub fn set_focus(&self, line: u64) {
        self.focus.store(line, Ordering::Relaxed);
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
    outcome: Settled,
    tuning: Tuning,
}

struct Plan {
    context: Context,
    batches: Vec<Batch>,
    left_lines: u64,
    right_lines: u64,
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
                self.post(DiffEvent::Done(settled));
                return;
            }
            restarts += 1;
            if !self.post(DiffEvent::Restarted) {
                let _first = self.outcome.set(Err(Reason::Stopped));
                return;
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
        for side in [&self.left, &self.right] {
            let _stale = side.source.take_changed() | side.source.take_rewritten();
        }
        let (left, right) = self.scan_phase()?;
        let plan = self.plan(left, right);
        let mut schedule = Schedule::new(plan.batches);
        self.sections_phase(&plan.context, &mut schedule)?;
        for side in [&plan.context.left, &plan.context.right] {
            side.source.probe_end().map_err(|_| Reason::ReadError)?;
            batch::unchanged(side)?;
        }
        Ok(Summary {
            left_lines: plan.left_lines,
            right_lines: plan.right_lines,
            sections: schedule.counts.sections,
            dropped_anchors: schedule.counts.dropped,
            restarts: 0,
        })
    }

    fn scan_phase(&self) -> Result<(ScanResult, ScanResult), Reason> {
        let total = self
            .left
            .source
            .len()
            .saturating_add(self.right.source.len());
        let meter = Meter {
            scanned: [AtomicU64::new(0), AtomicU64::new(0)],
            total,
        };
        thread::scope(|scope| {
            let left = scope.spawn(|| self.scan_side(&self.left, &meter, 0));
            let right = scope.spawn(|| self.scan_side(&self.right, &meter, 1));
            let provisional = self.provisional();
            let left = left.join().unwrap_or(Err(Reason::WorkerPanicked));
            let right = right.join().unwrap_or(Err(Reason::WorkerPanicked));
            provisional?;
            Ok((left?, right?))
        })
    }

    fn scan_side(
        &self,
        side: &LargeSide,
        meter: &Meter,
        slot: usize,
    ) -> Result<ScanResult, Reason> {
        let mut progress = |at: u64| {
            meter.scanned[slot].store(at, Ordering::Relaxed);
            let [left, right] = &meter.scanned;
            let done = left.load(Ordering::Relaxed) + right.load(Ordering::Relaxed);
            self.hint(Stage::Scan, done, meter.total);
        };
        let source = side.source.as_ref();
        scan::scan(
            source,
            side.layout,
            &self.options,
            BUCKETS,
            &self.cancel,
            &mut progress,
        )
    }

    fn provisional(&self) -> Result<(), Reason> {
        let stop = || self.cancel.is_cancelled();
        let left = read::head(&self.left, &stop)?;
        let right = read::head(&self.right, &stop)?;
        let left_set = left.set(self.left.layout.text.bom);
        let right_set = right.set(self.right.layout.text.bom);
        let hunks = diff_lines(left_set, right_set, &self.options).hunks;
        self.send(DiffEvent::Provisional(hunks))
    }

    fn plan(&self, mut left: ScanResult, mut right: ScanResult) -> Plan {
        if let Some(forge) = &self.tuning.forge {
            forge(&mut left.candidates, &mut right.candidates);
        }
        let anchors = anchor::anchors(&keyed(&left.candidates), &keyed(&right.candidates));
        let sections = anchor::sections(&anchors, left.lines, right.lines);
        let terminal = anchors.last().is_some_and(|last| {
            last.left.saturating_add(1) == left.lines && last.right.saturating_add(1) == right.lines
        });
        let (left_lines, right_lines) = (left.lines, right.lines);
        let side = |side: &LargeSide, scanned: ScanResult| SideData {
            source: Arc::clone(&side.source),
            layout: side.layout,
            len: scanned.index.scanned(),
            index: scanned.index,
        };
        Plan {
            context: Context {
                left: side(&self.left, left),
                right: side(&self.right, right),
                options: self.options,
            },
            batches: group(sections, self.tuning.batch_lines, terminal),
            left_lines,
            right_lines,
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

struct Meter {
    scanned: [AtomicU64; 2],
    total: u64,
}

fn keyed(candidates: &[scan::Candidate]) -> Vec<anchor::Candidate<u64>> {
    candidates
        .iter()
        .map(|candidate| anchor::Candidate {
            key: candidate.hash,
            position: u64::from(candidate.line),
        })
        .collect()
}

#[cfg(test)]
mod tests;
