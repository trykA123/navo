use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::model::{Generation, Reason, Row};
use crate::queue::Progress;

use super::rollup::Rollup;
use super::{LimitStop, RowBatch, Totals};

/// How often a stopped walk re-checks for cancel when no wake-up arrives.
const STOP_RECHECK: Duration = Duration::from_millis(250);

pub(super) struct Shared {
    state: Mutex<State>,
    resumed: Condvar,
    rows: Mutex<Vec<Option<Pending>>>,
    id_base: AtomicU64,
}

/// A row waiting for its content check, with the folder row it was listed under.
struct Pending {
    row: Row,
    parent: Option<u64>,
}

struct State {
    generation: Generation,
    totals: Totals,
    walk_done: bool,
    final_emitted: bool,
    closed: bool,
    stopped: Option<LimitStop>,
    rollup: Rollup,
    sender: Sender<RowBatch>,
}

impl Shared {
    pub(super) fn new(sender: Sender<RowBatch>, generation: Generation) -> Self {
        Self {
            state: Mutex::new(State {
                generation,
                totals: Totals::default(),
                walk_done: false,
                final_emitted: false,
                closed: false,
                stopped: None,
                rollup: Rollup::default(),
                sender,
            }),
            resumed: Condvar::new(),
            rows: Mutex::new(Vec::new()),
            id_base: AtomicU64::new(0),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn rows(&self) -> MutexGuard<'_, Vec<Option<Pending>>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn begin(&self, generation: Generation) {
        self.state().generation = generation;
    }

    pub(super) fn reset(&self, generation: Generation) {
        let mut state = self.state();
        state.generation = generation;
        state.totals = Totals::default();
        state.walk_done = false;
        state.final_emitted = false;
        state.closed = false;
        state.stopped = None;
        state.rollup = Rollup::default();
    }

    pub(super) fn reset_rows(&self, id_base: u64) {
        self.id_base.store(id_base, Ordering::Release);
        *self.rows() = Vec::new();
    }

    pub(super) fn close(&self) {
        self.state().closed = true;
    }

    pub(super) fn is_closed(&self) -> bool {
        self.state().closed
    }

    pub(super) fn store_rows(&self, rows: Vec<Row>, parent: Option<u64>) {
        let base = self.id_base.load(Ordering::Acquire);
        let mut store = self.rows();
        for row in rows {
            let Some(offset) = row.id.checked_sub(base) else {
                continue;
            };
            let Ok(index) = usize::try_from(offset) else {
                continue;
            };
            if store.len() <= index {
                store.resize_with(index.saturating_add(1), || None);
            }
            if let Some(slot) = store.get_mut(index) {
                *slot = Some(Pending { row, parent });
            }
        }
    }

    /// The row waiting under `id` and the folder row it was listed under.
    pub(super) fn take_row(&self, id: u64) -> Option<(Row, Option<u64>)> {
        let offset = id.checked_sub(self.id_base.load(Ordering::Acquire))?;
        let index = usize::try_from(offset).ok()?;
        let pending = self.rows().get_mut(index)?.take()?;
        Some((pending.row, pending.parent))
    }

    /// Sends the rows listed under the folder row `parent`, plus the ancestors they newly flag.
    pub(super) fn emit_initial(
        &self,
        generation: Generation,
        rows: Vec<Row>,
        parent: Option<u64>,
    ) -> bool {
        let counts = Totals::tally(&rows);
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.totals.merge(counts);
        let mut rows = rows;
        let flagged = state.rollup.add_batch(&rows, parent);
        rows.extend(flagged);
        let is_final = final_batch(&mut state);
        rows.extend(state.rollup.take_dirty(Instant::now(), is_final));
        send(&mut state, rows, is_final, None)
    }

    pub(super) fn emit_update(
        &self,
        generation: Generation,
        mut row: Row,
        previous: crate::model::Status,
    ) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.totals.replace(previous, row.raw_status);
        let flagged = state.rollup.update_folder(&mut row, previous);
        let is_final = final_batch(&mut state);
        let mut rows = vec![row];
        rows.extend(flagged);
        rows.extend(state.rollup.take_dirty(Instant::now(), is_final));
        send(&mut state, rows, is_final, None)
    }

    pub(super) fn emit_final(&self, generation: Generation, row: Row, parent: Option<u64>) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.totals.resolve(row.raw_status);
        let flagged = state.rollup.resolve(parent, row.raw_status);
        let is_final = final_batch(&mut state);
        let mut rows = vec![row];
        rows.extend(flagged);
        rows.extend(state.rollup.take_dirty(Instant::now(), is_final));
        send(&mut state, rows, is_final, None)
    }

    pub(super) fn emit_progress(&self, generation: Generation, progress: Progress) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        let batch = RowBatch {
            generation: state.generation,
            rows: Vec::new(),
            progress: vec![progress],
            totals: state.totals,
            is_final: false,
            problem: None,
            limit: state.stopped,
        };
        send_batch(&mut state, batch)
    }

    /// Marks the walk as stopped at the limit and tells the receiver.
    pub(super) fn stop_at_limit(&self, generation: Generation, stop: LimitStop) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.stopped = Some(stop);
        let rows = state.rollup.take_dirty(Instant::now(), false);
        send(&mut state, rows, false, None)
    }

    /// Waits until `resume`. False when `abandon` says the walk is over or the session closed.
    pub(super) fn wait_resumed(&self, generation: Generation, abandon: impl Fn() -> bool) -> bool {
        let mut state = self.state();
        while state.stopped.is_some() {
            if abandon() || state.closed || state.generation != generation {
                return false;
            }
            state = self
                .resumed
                .wait_timeout(state, STOP_RECHECK)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        !state.closed && state.generation == generation
    }

    /// Ends a stop; false when the walk of `generation` is not stopped.
    pub(super) fn resume(&self, generation: Generation) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed || state.stopped.take().is_none() {
            return false;
        }
        let rows = state.rollup.take_dirty(Instant::now(), false);
        let sent = send(&mut state, rows, false, None);
        self.resumed.notify_all();
        sent
    }

    /// Wakes a stopped walk so it sees a cancel.
    pub(super) fn wake(&self) {
        let _state = self.state();
        self.resumed.notify_all();
    }

    pub(super) fn finish(&self, generation: Generation) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.walk_done = true;
        state.stopped = None;
        if state.totals.checking > 0 || state.final_emitted {
            return true;
        }
        state.final_emitted = true;
        let rows = state.rollup.take_dirty(Instant::now(), true);
        send(&mut state, rows, true, None)
    }

    pub(super) fn finish_problem(&self, generation: Generation, reason: Reason) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.walk_done = true;
        state.totals.unavailable = state
            .totals
            .unavailable
            .saturating_add(state.totals.checking);
        state.totals.checking = 0;
        if state.final_emitted {
            return true;
        }
        state.final_emitted = true;
        let rows = state.rollup.take_dirty(Instant::now(), true);
        let sent = send(&mut state, rows, true, Some(reason));
        state.closed = true;
        sent
    }

    pub(super) fn report_problem(&self, generation: Generation, reason: Reason) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        send(&mut state, Vec::new(), false, Some(reason))
    }

    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        let store = self.rows();
        let mut bytes = store.capacity() * std::mem::size_of::<Option<Pending>>();
        for row in store.iter().flatten().map(|pending| &pending.row) {
            for entry in [row.left.as_deref(), row.right.as_deref()]
                .into_iter()
                .flatten()
            {
                bytes = bytes
                    .saturating_add(2 * std::mem::size_of::<usize>())
                    .saturating_add(std::mem::size_of::<crate::model::SideEntry>())
                    .saturating_add(path_bytes(entry.rel.as_path()));
            }
        }
        bytes
    }
}

#[cfg(test)]
fn path_bytes(path: &navo_core::ProviderPath) -> usize {
    if path.as_str().is_some() {
        return path.to_text().capacity();
    }
    if let Some(wide) = path.to_wide() {
        return wide.capacity() * std::mem::size_of::<u16>();
    }
    path.to_bytes().map_or(0, |bytes| bytes.capacity())
}

fn final_batch(state: &mut State) -> bool {
    let is_final = state.walk_done && state.totals.checking == 0 && !state.final_emitted;
    if is_final {
        state.final_emitted = true;
    }
    is_final
}

fn send(state: &mut State, rows: Vec<Row>, is_final: bool, problem: Option<Reason>) -> bool {
    let batch = RowBatch {
        generation: state.generation,
        rows,
        progress: Vec::new(),
        totals: state.totals,
        is_final,
        problem,
        limit: state.stopped,
    };
    send_batch(state, batch)
}

fn send_batch(state: &mut State, batch: RowBatch) -> bool {
    match state.sender.send(batch) {
        Ok(()) => true,
        Err(_) => {
            state.closed = true;
            false
        }
    }
}
