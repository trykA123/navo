//! `LocalCopyHost` (C3): the app's `CopyHost` over the write journal. A copy runs on a
//! `compare-copy` thread and an undo on `compare-undo`; one write runs at a time, and the
//! session's committed copies form the undo stack, newest last (NC148, NC166).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use navo_compare::text::Direction;
use navo_ui::compare_diff::{
    CopyChange, CopyDirection, CopyDone, CopyResult, CopySink, SheetPlan, SheetPlanSink, SheetSink,
    UndoDone, UndoSink, UndoStart,
};
use sidewise_journal::{Journal, JournalError, UndoError, WriteError};

use super::job::{Committed, copy_now};
use super::request::{CopyRefusal, CopyRequest};
use crate::compare::diff::copy_port::{CopyHost, refuse_sheet};

#[cfg(test)]
mod tests;

/// The blocking copy a host runs on its thread; `copy_now` outside tests.
pub(crate) type CopyFn = fn(&Journal, &[CopyRequest]) -> Result<Committed, CopyRefusal>;

/// What the host and its threads share.
struct State {
    journal: Arc<Journal>,
    /// Committed copies of this session, newest last; undo jobs are never pushed.
    undo: Mutex<Vec<Committed>>,
    /// True while a copy or an undo runs, so the stack changes in one order only.
    busy: AtomicBool,
    copy: CopyFn,
}

impl State {
    /// Push a committed copy on the undo stack. A poisoned lock still holds a whole stack.
    fn push(&self, committed: Committed) {
        self.undo
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(committed);
    }

    /// Pop the newest committed copy when it is `job` (any for `None`). `Err(NotNewest)` when
    /// `job` is on the stack under a newer copy, `Err(Gone)` when it is not on the stack
    /// (undone, or dropped), `Ok(None)` for an empty stack.
    fn pop_if(&self, job: Option<&str>) -> Result<Option<Committed>, UndoStart> {
        let mut stack = self.undo.lock().unwrap_or_else(PoisonError::into_inner);
        match (stack.last(), job) {
            (Some(top), Some(job)) if top.job.as_str() == job => Ok(stack.pop()),
            (_, Some(job)) if stack.iter().any(|entry| entry.job.as_str() == job) => {
                Err(UndoStart::NotNewest)
            }
            (_, Some(_)) => Err(UndoStart::Gone),
            (_, None) => Ok(stack.pop()),
        }
    }
}

/// The held write slot; dropping it frees the slot, on a panic too.
pub(crate) struct Busy(Arc<State>);

impl Busy {
    /// Takes the write slot, or `None` while another write holds it.
    fn take(state: &Arc<State>) -> Option<Self> {
        state
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(Arc::clone(state)))
    }
}

impl Drop for Busy {
    /// Frees the write slot.
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

/// Writes copies into local files through the journal and undoes them, newest first.
pub struct LocalCopyHost {
    state: Arc<State>,
}

impl LocalCopyHost {
    /// A host whose copies and undos write through `journal`, with an empty undo stack.
    pub fn new(journal: Arc<Journal>) -> Self {
        Self::with_copy(journal, copy_now)
    }

    /// A host that runs `copy` for each copy; tests pass one that panics.
    pub(crate) fn with_copy(journal: Arc<Journal>, copy: CopyFn) -> Self {
        let state = State {
            journal,
            undo: Mutex::new(Vec::new()),
            busy: AtomicBool::new(false),
            copy,
        };
        Self {
            state: Arc::new(state),
        }
    }

    /// Holds the write slot as a running write would.
    #[cfg(test)]
    pub(crate) fn try_busy(&self) -> Option<Busy> {
        Busy::take(&self.state)
    }

    /// Copy `changes` of one pair as one job on a `compare-copy` thread and report once
    /// through `done`; a refusal when there are no changes, another write runs or the thread
    /// cannot start.
    fn start_copy(&self, changes: Vec<CopyChange>, done: CopySink) {
        let Some(first) = changes.first() else {
            done(CopyDone {
                request: 0,
                direction: CopyDirection::ToRight,
                result: CopyResult::Refused(CopyRefusal::NotCopyable.text()),
                job: None,
            });
            return;
        };
        let (request, direction) = (first.pair.id, first.direction);
        let answer = move |(result, job)| CopyDone {
            request,
            direction,
            result,
            job,
        };
        let Some(busy) = Busy::take(&self.state) else {
            done(answer((running(), None)));
            return;
        };
        let state = Arc::clone(&self.state);
        let report = done.clone();
        let spawned = thread::Builder::new()
            .name("compare-copy".to_owned())
            .spawn(move || {
                guarded(
                    done,
                    move || answer((stopped(), None)),
                    busy,
                    move || answer(copy_with(&state, &changes)),
                );
            });
        if let Err(error) = spawned {
            report(answer((
                CopyResult::Refused(CopyRefusal::Failed(error.to_string()).text()),
                None,
            )));
        }
    }
}

impl CopyHost for LocalCopyHost {
    /// Copy one change on a new thread; see `start_copy`.
    fn copy_change(&self, change: CopyChange, done: CopySink) {
        self.start_copy(vec![change], done);
    }

    /// Copy several changes of one pair as one job on a new thread; see `start_copy`.
    fn copy_changes(&self, changes: Vec<CopyChange>, done: CopySink) {
        self.start_copy(changes, done);
    }

    /// Plans no file until the sheet writes in P1b.
    fn plan_sheet(&self, _changes: Vec<CopyChange>, planned: SheetPlanSink) {
        planned(SheetPlan::default());
    }

    /// Refuses until the sheet writes in P1b.
    fn run_sheet(&self, done: SheetSink) {
        refuse_sheet(done);
    }

    /// Pop the newest copy and undo it on a `compare-undo` thread. While a write runs it
    /// reports "running" instead. A failed undo goes back on the stack unless it can never work
    /// (NC169: a changed target, or an undo in doubt).
    fn undo_last(&self, job: Option<&str>, done: UndoSink) -> UndoStart {
        let Some(busy) = Busy::take(&self.state) else {
            done(UndoDone {
                targets: Vec::new(),
                result: running(),
            });
            return UndoStart::Started;
        };
        // The held write slot keeps copies from pushing between this check and the pop.
        let entry = match self.state.pop_if(job) {
            Ok(Some(entry)) => entry,
            Ok(None) => return UndoStart::Nothing,
            Err(start) => return start,
        };
        let (state, kept, report) = (Arc::clone(&self.state), entry.clone(), done.clone());
        let on_panic = (Arc::clone(&self.state), entry.clone());
        let spawned = thread::Builder::new()
            .name("compare-undo".to_owned())
            .spawn(move || {
                let refused = move || {
                    on_panic.0.push(on_panic.1.clone());
                    undo_done(&on_panic.1, undo_refused("the undo stopped unexpectedly"))
                };
                guarded(done, refused, busy, move || undo_now(&state, entry));
            });
        if let Err(error) = spawned {
            self.state.push(kept.clone());
            let reason = error.to_string();
            report(undo_done(&kept, undo_refused(&reason)));
        }
        UndoStart::Started
    }
}

/// Run the copy of `changes` now and push the committed job on the undo stack; returns the
/// result and, for a commit, its job id.
fn copy_with(state: &State, changes: &[CopyChange]) -> (CopyResult, Option<String>) {
    let requests: Vec<CopyRequest> = changes.iter().map(request_of).collect();
    match (state.copy)(&state.journal, &requests) {
        Ok(committed) => {
            let job = committed.job.as_str().to_owned();
            state.push(committed);
            (CopyResult::Copied, Some(job))
        }
        Err(refusal) => (result_of(&refusal), None),
    }
}

/// The answer for a refused copy: `Unconfirmed` when the target may hold new bytes (NC168).
fn result_of(refusal: &CopyRefusal) -> CopyResult {
    match refusal {
        CopyRefusal::Unconfirmed => CopyResult::Unconfirmed(refusal.text()),
        other => CopyResult::Refused(other.text()),
    }
}

/// Undo `entry` now and tell how it went; puts it back on the stack when a retry can work.
/// A changed target or an undo in doubt drops the entry (NC169); older entries re-check theirs.
fn undo_now(state: &State, entry: Committed) -> UndoDone {
    let Err(error) = state.journal.undo(&entry.job) else {
        return undo_done(&entry, CopyResult::Copied);
    };
    let result = match &error {
        UndoError::Changed(_) | UndoError::Write(WriteError::Changed) => {
            CopyResult::Refused(navo_l10n::tr("compare-undo-changed"))
        }
        UndoError::Journal(JournalError::InDoubt(_))
        | UndoError::Write(WriteError::NotRolledBack { .. }) => {
            result_of(&CopyRefusal::Unconfirmed)
        }
        other => undo_refused(&other.to_string()),
    };
    let lasting = matches!(
        error,
        UndoError::NotFound
            | UndoError::NotUndoable(_)
            | UndoError::Damaged(_)
            | UndoError::Unsupported
            | UndoError::Changed(_)
            | UndoError::Write(WriteError::Changed | WriteError::NotRolledBack { .. })
            | UndoError::Journal(JournalError::InDoubt(_))
    );
    tracing::warn!(job = entry.job.as_str(), lasting, "undo refused");
    let done = undo_done(&entry, result);
    if !lasting {
        state.push(entry);
    }
    done
}

/// The end of the undo of `entry`.
fn undo_done(entry: &Committed, result: CopyResult) -> UndoDone {
    UndoDone {
        targets: vec![entry.target.clone()],
        result,
    }
}

/// A failed undo, with its reason.
fn undo_refused(reason: &str) -> CopyResult {
    CopyResult::Refused(navo_l10n::t!("compare-undo-failed", reason = reason))
}

/// The refusal while another copy or undo runs.
fn running() -> CopyResult {
    CopyResult::Refused(navo_l10n::tr("compare-copy-running"))
}

/// The refusal for a write thread that stopped without an answer (a panic).
fn stopped() -> CopyResult {
    let why = CopyRefusal::Failed("the copy stopped unexpectedly".to_owned());
    CopyResult::Refused(why.text())
}

/// The engine's direction for the UI's.
fn direction_of(direction: CopyDirection) -> Direction {
    match direction {
        CopyDirection::ToRight => Direction::LeftToRight,
        CopyDirection::ToLeft => Direction::RightToLeft,
    }
}

/// The request the copy runs for one change the UI showed.
fn request_of(change: &CopyChange) -> CopyRequest {
    CopyRequest {
        pair: change.pair.clone(),
        left_bytes: change.left_bytes.clone(),
        right_bytes: change.right_bytes.clone(),
        digest: change.digest,
        direction: direction_of(change.direction),
        lines: change.lines.clone(),
    }
}

/// Sends a refusal when dropped before the work answered, so a thread that panics still ends
/// the UI's wait (NV `EndGuard`). It holds the write slot until the refusal is built.
struct EndGuard<T, R: FnOnce() -> T> {
    sink: Option<Arc<dyn Fn(T) + Send + Sync>>,
    refused: Option<R>,
    busy: Option<Busy>,
}

impl<T, R: FnOnce() -> T> Drop for EndGuard<T, R> {
    /// Reports the refusal when the work ended without a result (a panic); the slot is freed
    /// after `refused` ran (it may push an entry back) and before the report.
    fn drop(&mut self) {
        if let (Some(sink), Some(refused)) = (self.sink.take(), self.refused.take()) {
            let result = refused();
            drop(self.busy.take());
            sink(result);
        }
    }
}

/// Runs `work` holding the write slot `busy` and reports its result through `sink`; if `work`
/// panics, reports `refused()` while the panic unwinds. Exactly one report is sent either way,
/// and the slot is free before it.
fn guarded<T, R: FnOnce() -> T>(
    sink: Arc<dyn Fn(T) + Send + Sync>,
    refused: R,
    busy: Busy,
    work: impl FnOnce() -> T,
) {
    let mut guard = EndGuard {
        sink: Some(sink),
        refused: Some(refused),
        busy: Some(busy),
    };
    let result = work();
    drop(guard.busy.take());
    if let Some(sink) = guard.sink.take() {
        sink(result);
    }
}
