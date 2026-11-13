use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use crate::model::{
    CancelToken, EntryKind, Generation, HashCache, ReadHint, Reason, RelPath, Row, SideEntry,
    SideSource, Status,
};
use crate::queue::{self, Progress, Queue};
use crate::rules::IgnoreRules;

mod classify;
mod limit;
mod rollup;
mod run;
mod shared;
mod totals;

pub use limit::{LimitStop, NetworkCap, StopBound};
pub use totals::Totals;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    Content,
    SizeTime,
}

/// Stage of a compare session currently reporting telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparePhase {
    Listing,
    QuickCheck,
    ContentCheck,
}

/// Progress counters for one phase of a compare walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseTelemetry {
    pub phase: ComparePhase,
    pub done: u64,
    pub total: Option<u64>,
}

/// Entry whose listing or content check met or exceeded the slow threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlowEntry {
    pub rel: RelPath,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowBatch {
    pub generation: Generation,
    pub rows: Vec<Row>,
    pub progress: Vec<Progress>,
    pub totals: Totals,
    pub is_final: bool,
    pub problem: Option<Reason>,
    /// Set on every batch while the walk waits for `Session::continue_walk`.
    pub limit: Option<LimitStop>,
    pub phase: Option<PhaseTelemetry>,
    pub slow: Vec<SlowEntry>,
}

impl Default for RowBatch {
    fn default() -> Self {
        Self {
            generation: Generation::new(0),
            rows: Vec::new(),
            progress: Vec::new(),
            totals: Totals::default(),
            is_final: false,
            problem: None,
            limit: None,
            phase: None,
            slow: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    StaleGeneration,
    Checking,
    Link,
    Submodule,
    Unavailable(Reason),
    NoRow,
    MultipleRows,
    NoSource,
    NotAFile,
    ReadOnlyTarget,
    TypeConflict,
    CaseConflict,
    NameUnwritable(NameProblem),
}

/// Why a name cannot be written on a target side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    DotName,
    Nul,
    TooLong,
    TrailingDotOrSpace,
    Reserved,
    IllegalCharacter,
}

#[derive(Clone)]
pub struct SessionOptions {
    pub rules: Arc<IgnoreRules>,
    pub cache: Arc<dyn HashCache>,
    pub read_hint: ReadHint,
    pub check_mode: CheckMode,
    /// Bounds a walk when a side is on a network drive; `None` means unlimited.
    pub network_cap: Option<NetworkCap>,
}

pub struct Session {
    left: Arc<dyn SideSource>,
    right: Arc<dyn SideSource>,
    options: SessionOptions,
    shared: Arc<shared::Shared>,
    queue: Arc<Queue>,
    next_row_id: Arc<AtomicU64>,
    row_id_base: u64,
    #[cfg(test)]
    settle_hook: Option<run::SettleHook>,
    cancel: CancelToken,
    receiver: Receiver<RowBatch>,
    handles: Vec<JoinHandle<()>>,
    generation: Generation,
}

impl Session {
    #[must_use]
    pub fn start(
        left: Arc<dyn SideSource>,
        right: Arc<dyn SideSource>,
        options: SessionOptions,
    ) -> Self {
        #[cfg(test)]
        {
            Self::build(left, right, options, None)
        }
        #[cfg(not(test))]
        {
            Self::build(left, right, options)
        }
    }

    #[cfg(test)]
    pub(crate) fn start_with_settle_hook(
        left: Arc<dyn SideSource>,
        right: Arc<dyn SideSource>,
        options: SessionOptions,
        settle_hook: run::SettleHook,
    ) -> Self {
        Self::build(left, right, options, Some(settle_hook))
    }

    fn build(
        left: Arc<dyn SideSource>,
        right: Arc<dyn SideSource>,
        options: SessionOptions,
        #[cfg(test)] settle_hook: Option<run::SettleHook>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let generation = Generation::new(1);
        let shared = Arc::new(shared::Shared::new(sender, generation));
        let queue = Arc::new(Queue::new(queue::worker_limit()));
        let next_row_id = Arc::new(AtomicU64::new(0));
        let cancel = CancelToken::new();
        let mut session = Self {
            left,
            right,
            options,
            shared,
            queue,
            next_row_id,
            row_id_base: 0,
            #[cfg(test)]
            settle_hook,
            cancel,
            receiver,
            handles: Vec::new(),
            generation,
        };
        session.spawn();
        session
    }

    #[must_use]
    pub fn batches(&self) -> &Receiver<RowBatch> {
        &self.receiver
    }

    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Overrides the slow-entry threshold in milliseconds for tests.
    #[cfg(test)]
    pub fn set_slow_threshold_ms_for_test(&self, ms: u64) {
        self.shared.set_slow_threshold_ms(ms);
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
        self.queue.stop_all();
        self.queue.close();
        self.shared.wake();
    }

    /// Resumes a walk that stopped at the network limit, where it stopped.
    /// Returns false when the walk is not stopped.
    pub fn continue_walk(&self) -> bool {
        self.shared.resume(self.generation)
    }

    pub fn stop_row(&self, id: u64) -> bool {
        if id < self.row_id_base {
            return false;
        }
        self.queue.stop(id)
    }

    pub fn stop_all(&self) {
        self.queue.stop_all();
    }

    pub fn focus(&self, row_ids: &[u64]) -> bool {
        if row_ids.iter().any(|id| *id < self.row_id_base) {
            return false;
        }
        self.queue.focus(row_ids);
        true
    }

    pub fn refresh(&mut self) {
        self.cancel.cancel();
        self.queue.stop_all();
        self.queue.close();
        let next = Generation::new(self.generation.get().saturating_add(1));
        self.shared.begin(next);
        self.shared.wake();
        let joined = self.join();
        while self.receiver.try_recv().is_ok() {}
        self.shared.reset(next);
        self.row_id_base = self.next_row_id.load(Ordering::Acquire);
        self.shared.reset_rows(self.row_id_base);
        self.generation = next;
        self.cancel = CancelToken::new();
        self.queue = Arc::new(Queue::new(queue::worker_limit()));
        if !joined {
            self.shared.report_problem(next, Reason::WorkerPanicked);
        }
        self.spawn();
    }

    pub fn write_allowed(&self, row: &Row, generation: Generation) -> Result<(), Refusal> {
        if generation != self.generation {
            return Err(Refusal::StaleGeneration);
        }
        row_write_allowed(row, generation)
    }

    fn spawn(&mut self) {
        self.handles = run::spawn(run::Run {
            left: Arc::clone(&self.left),
            right: Arc::clone(&self.right),
            options: self.options.clone(),
            generation: self.generation,
            cancel: self.cancel.clone(),
            shared: Arc::clone(&self.shared),
            queue: Arc::clone(&self.queue),
            next_row_id: Arc::clone(&self.next_row_id),
            #[cfg(test)]
            settle_hook: self.settle_hook.clone(),
        });
    }

    fn join(&mut self) -> bool {
        let mut joined = true;
        for handle in self.handles.drain(..) {
            if handle.join().is_err() {
                joined = false;
            }
        }
        joined
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.queue.stop_all();
        self.queue.close();
        self.shared.wake();
        if !self.join() {
            self.shared.close();
        }
    }
}

pub(crate) fn row_write_allowed(row: &Row, generation: Generation) -> Result<(), Refusal> {
    if row.generation != generation {
        return Err(Refusal::StaleGeneration);
    }
    if row.raw_status == Status::Checking || row.display_status == Status::Checking {
        return Err(Refusal::Checking);
    }
    if kind_of(row.left.as_deref(), EntryKind::Link)
        || kind_of(row.right.as_deref(), EntryKind::Link)
    {
        return Err(Refusal::Link);
    }
    if kind_of(row.left.as_deref(), EntryKind::Submodule)
        || kind_of(row.right.as_deref(), EntryKind::Submodule)
    {
        return Err(Refusal::Submodule);
    }
    for status in [row.raw_status, row.display_status] {
        if let Status::Unavailable(reason) = status {
            return Err(Refusal::Unavailable(reason));
        }
    }
    Ok(())
}

fn kind_of(entry: Option<&SideEntry>, kind: EntryKind) -> bool {
    entry.is_some_and(|entry| entry.kind == kind)
}

#[cfg(test)]
mod tests;
