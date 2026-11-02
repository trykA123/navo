use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use crate::model::{
    CancelToken, EntryKind, Generation, HashCache, ReadHint, Reason, Row, SideEntry, SideSource,
    Status,
};
use crate::queue::{self, Progress, Queue};
use crate::rules::IgnoreRules;

mod classify;
mod run;
mod shared;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    Content,
    SizeTime,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub rows: u64,
    pub same: u64,
    pub different: u64,
    pub only_left: u64,
    pub only_right: u64,
    pub conflicts: u64,
    pub checking: u64,
    pub same_size_time: u64,
    pub unavailable: u64,
    pub not_checked: u64,
}

impl Totals {
    pub(super) fn record(&mut self, status: Status, first: bool) {
        if first {
            self.rows = self.rows.saturating_add(1);
        }
        match status {
            Status::Same | Status::SameAfterRules => {
                self.same = self.same.saturating_add(1);
            }
            Status::Different { .. } => {
                self.different = self.different.saturating_add(1);
            }
            Status::OnlyLeft => {
                self.only_left = self.only_left.saturating_add(1);
            }
            Status::OnlyRight => {
                self.only_right = self.only_right.saturating_add(1);
            }
            Status::TypeConflict | Status::CaseConflict => {
                self.conflicts = self.conflicts.saturating_add(1);
            }
            Status::Checking => {
                self.checking = self.checking.saturating_add(1);
            }
            Status::SameSizeTime => {
                self.same_size_time = self.same_size_time.saturating_add(1);
            }
            Status::Unavailable(_) => {
                self.unavailable = self.unavailable.saturating_add(1);
            }
            Status::NotChecked(_) => {
                self.not_checked = self.not_checked.saturating_add(1);
            }
        }
    }

    pub(super) fn resolve(&mut self, status: Status) {
        self.checking = self.checking.saturating_sub(1);
        self.record(status, false);
    }

    pub(super) fn tally(rows: &[Row]) -> Self {
        let mut totals = Self::default();
        for row in rows {
            totals.record(row.raw_status, true);
        }
        totals
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.rows = self.rows.saturating_add(other.rows);
        self.same = self.same.saturating_add(other.same);
        self.different = self.different.saturating_add(other.different);
        self.only_left = self.only_left.saturating_add(other.only_left);
        self.only_right = self.only_right.saturating_add(other.only_right);
        self.conflicts = self.conflicts.saturating_add(other.conflicts);
        self.checking = self.checking.saturating_add(other.checking);
        self.same_size_time = self.same_size_time.saturating_add(other.same_size_time);
        self.unavailable = self.unavailable.saturating_add(other.unavailable);
        self.not_checked = self.not_checked.saturating_add(other.not_checked);
    }

    pub(super) fn replace(&mut self, previous: Status, next: Status) {
        self.remove(previous);
        self.record(next, false);
    }

    fn remove(&mut self, status: Status) {
        match status {
            Status::Same | Status::SameAfterRules => {
                self.same = self.same.saturating_sub(1);
            }
            Status::Different { .. } => {
                self.different = self.different.saturating_sub(1);
            }
            Status::OnlyLeft => {
                self.only_left = self.only_left.saturating_sub(1);
            }
            Status::OnlyRight => {
                self.only_right = self.only_right.saturating_sub(1);
            }
            Status::TypeConflict | Status::CaseConflict => {
                self.conflicts = self.conflicts.saturating_sub(1);
            }
            Status::Checking => {
                self.checking = self.checking.saturating_sub(1);
            }
            Status::SameSizeTime => {
                self.same_size_time = self.same_size_time.saturating_sub(1);
            }
            Status::Unavailable(_) => {
                self.unavailable = self.unavailable.saturating_sub(1);
            }
            Status::NotChecked(_) => {
                self.not_checked = self.not_checked.saturating_sub(1);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowBatch {
    pub generation: Generation,
    pub rows: Vec<Row>,
    pub progress: Vec<Progress>,
    pub totals: Totals,
    pub is_final: bool,
    pub problem: Option<Reason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    StaleGeneration,
    Checking,
    Link,
    Submodule,
    Unavailable(Reason),
}

#[derive(Clone)]
pub struct SessionOptions {
    pub rules: Arc<IgnoreRules>,
    pub cache: Arc<dyn HashCache>,
    pub read_hint: ReadHint,
    pub check_mode: CheckMode,
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

    pub fn cancel(&self) {
        self.cancel.cancel();
        self.queue.stop_all();
        self.queue.close();
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
        if generation != self.generation || row.generation != generation {
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
        if !self.join() {
            self.shared.close();
        }
    }
}

fn kind_of(entry: Option<&SideEntry>, kind: EntryKind) -> bool {
    entry.is_some_and(|entry| entry.kind == kind)
}

#[cfg(test)]
mod tests;
