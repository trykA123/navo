//! State the worker and the callers share.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::model::Saved;
use crate::store::Stored;

use super::{FailKind, NoteHeader, Snapshot};

type Listener = Box<dyn Fn(u64) + Send>;

fn same_rows(a: &[Stored], b: &[Stored]) -> bool {
    let key = |row: &Stored| (row.id, row.entry.last_ms, row.entry.uses);
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| key(x) == key(y))
}

/// The parts of a snapshot the worker read again; `None` keeps the current part.
#[derive(Default)]
pub(super) struct Fresh {
    pub(super) history: Option<Vec<Stored>>,
    pub(super) saved: Option<(Vec<Saved>, usize)>,
    pub(super) notes: Option<(Vec<NoteHeader>, usize)>,
}

impl Fresh {
    /// The next snapshot, or `None` when nothing changed.
    fn apply(self, current: &Snapshot) -> Option<Snapshot> {
        let history = self
            .history
            .filter(|rows| !same_rows(&current.history, rows));
        let saved = self.saved.filter(|(items, skipped)| {
            (items, *skipped) != (&current.saved, current.saved_skipped)
        });
        let notes = self.notes.filter(|(notes, skipped)| {
            (notes, *skipped) != (&current.notes, current.notes_skipped)
        });
        if history.is_none() && saved.is_none() && notes.is_none() {
            return None;
        }
        let (saved, saved_skipped) =
            saved.unwrap_or_else(|| (current.saved.clone(), current.saved_skipped));
        let (notes, notes_skipped) =
            notes.unwrap_or_else(|| (current.notes.clone(), current.notes_skipped));
        Some(Snapshot {
            revision: current.revision + 1,
            history: history.unwrap_or_else(|| current.history.clone()),
            saved,
            saved_skipped,
            notes,
            notes_skipped,
        })
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Marks the worker as ended when its thread leaves, also by a panic.
pub(super) struct EndGuard(pub(super) Arc<Shared>);

impl Drop for EndGuard {
    fn drop(&mut self) {
        self.0.set_ended();
    }
}

pub(super) struct Shared {
    snapshot: Mutex<Arc<Snapshot>>,
    listeners: Mutex<Vec<Arc<Mutex<Listener>>>>,
    recording: AtomicBool,
    stopping: AtomicBool,
    ended: AtomicBool,
    dropped: AtomicU64,
    errors: AtomicU64,
    last_error: Mutex<Option<FailKind>>,
}

impl Shared {
    pub(super) fn new() -> Self {
        Self {
            snapshot: Mutex::new(Arc::new(Snapshot::empty())),
            listeners: Mutex::new(Vec::new()),
            recording: AtomicBool::new(true),
            stopping: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            last_error: Mutex::new(None),
        }
    }

    pub(super) fn snapshot(&self) -> Arc<Snapshot> {
        lock(&self.snapshot).clone()
    }

    /// Swaps in the fresh parts with the next revision and tells the listeners.
    /// History rows with the same ids, times and use counts count as unchanged: a row never
    /// changes otherwise.
    pub(super) fn publish(&self, fresh: Fresh) {
        let revision = {
            let mut current = lock(&self.snapshot);
            let Some(next) = fresh.apply(&current) else {
                return;
            };
            *current = Arc::new(next);
            current.revision
        };
        let listeners = lock(&self.listeners).clone();
        for listener in listeners {
            let call = || (*lock(&listener))(revision);
            if catch_unwind(AssertUnwindSafe(call)).is_err() {
                self.fail(FailKind::Thread);
            }
        }
    }

    pub(super) fn subscribe(&self, listener: Listener) {
        lock(&self.listeners).push(Arc::new(Mutex::new(listener)));
    }

    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub(super) fn set_stopping(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    pub(super) fn set_ended(&self) {
        self.ended.store(true, Ordering::Release);
    }

    pub(super) fn has_ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }

    pub(super) fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Acquire)
    }

    pub(super) fn set_recording(&self, on: bool) {
        self.recording.store(on, Ordering::Release);
    }

    pub(super) fn count_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub(super) fn fail(&self, kind: FailKind) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        *lock(&self.last_error) = Some(kind);
    }

    pub(super) fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub(super) fn last_error(&self) -> Option<FailKind> {
        *lock(&self.last_error)
    }
}
