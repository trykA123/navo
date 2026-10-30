use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::model::{Generation, Reason, Row};
use crate::queue::Progress;

use super::{RowBatch, Totals};

pub(super) struct Shared {
    state: Mutex<State>,
    rows: Mutex<Vec<Option<Row>>>,
    id_base: AtomicU64,
}

struct State {
    generation: Generation,
    totals: Totals,
    walk_done: bool,
    final_emitted: bool,
    closed: bool,
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
                sender,
            }),
            rows: Mutex::new(Vec::new()),
            id_base: AtomicU64::new(0),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn rows(&self) -> MutexGuard<'_, Vec<Option<Row>>> {
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

    pub(super) fn store_rows(&self, rows: Vec<Row>) {
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
                store.resize(index.saturating_add(1), None);
            }
            if let Some(slot) = store.get_mut(index) {
                *slot = Some(row);
            }
        }
    }

    pub(super) fn take_row(&self, id: u64) -> Option<Row> {
        let offset = id.checked_sub(self.id_base.load(Ordering::Acquire))?;
        let index = usize::try_from(offset).ok()?;
        self.rows().get_mut(index)?.take()
    }

    pub(super) fn emit_initial(&self, generation: Generation, rows: Vec<Row>) -> bool {
        let counts = Totals::tally(&rows);
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.totals.merge(counts);
        let is_final = final_batch(&mut state);
        send(&mut state, rows, is_final, None)
    }

    pub(super) fn emit_update(
        &self,
        generation: Generation,
        row: Row,
        previous: crate::model::Status,
    ) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.totals.replace(previous, row.raw_status);
        let is_final = final_batch(&mut state);
        send(&mut state, vec![row], is_final, None)
    }

    pub(super) fn emit_final(&self, generation: Generation, row: Row) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.totals.resolve(row.raw_status);
        let is_final = final_batch(&mut state);
        send(&mut state, vec![row], is_final, None)
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
        };
        send_batch(&mut state, batch)
    }

    pub(super) fn finish(&self, generation: Generation) -> bool {
        let mut state = self.state();
        if state.generation != generation || state.closed {
            return false;
        }
        state.walk_done = true;
        if state.totals.checking > 0 || state.final_emitted {
            return true;
        }
        state.final_emitted = true;
        send(&mut state, Vec::new(), true, None)
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
        let sent = send(&mut state, Vec::new(), true, Some(reason));
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
        let mut bytes = store.capacity() * std::mem::size_of::<Option<Row>>();
        for row in store.iter().flatten() {
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
fn path_bytes(path: &rusty_core::ProviderPath) -> usize {
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
