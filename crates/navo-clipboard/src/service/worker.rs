//! The thread that owns the store and handles one message at a time.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};

use crate::admit::admit;
use crate::limits::Limits;
use crate::model::{Captured, Content, Transfer};
use crate::store::{ClipError, ClipStore};

use super::notes::NoteMsg;
use super::saved::SavedMsg;
use super::secret::ClearTimer;
use super::shared::{Fresh, Shared};
use super::{FailKind, Ports};

pub(super) enum Msg {
    Captured(Captured),
    Copy(i64, Option<Sender<Result<(), FailKind>>>),
    Delete(i64),
    Clear(Sender<Result<(), FailKind>>),
    Wipe(Sender<Result<(), FailKind>>),
    Saved(SavedMsg),
    Note(NoteMsg),
    #[cfg(test)]
    Wake,
}

pub(super) struct Worker {
    pub(super) store: Option<ClipStore>,
    path: PathBuf,
    pub(super) limits: Limits,
    pub(super) ports: Ports,
    pub(super) shared: Arc<Shared>,
    pub(super) timer: ClearTimer,
}

impl Worker {
    pub(super) fn new(store: ClipStore, limits: Limits, ports: Ports, shared: Arc<Shared>) -> Self {
        Self {
            path: store.path().to_path_buf(),
            store: Some(store),
            limits,
            ports,
            shared,
            timer: ClearTimer::default(),
        }
    }

    /// Handles messages until every sender is gone and the queue is empty. Waits no longer
    /// than the pending secret's next attempt, then empties the clipboard when it is due.
    pub(super) fn run(mut self, rx: &Receiver<Msg>) {
        loop {
            let received = match self.timer.wait(self.ports.clock.now()) {
                Some(wait) => rx.recv_timeout(wait),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match received {
                Ok(msg) if !self.shared.is_stopping() => self.handle(msg),
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if self.timer.is_due(self.ports.clock.now()) {
                self.clear_pending();
            }
        }
    }

    /// Reads every part again, as after an open or a wipe.
    pub(super) fn refresh(&mut self) {
        let fresh = Fresh {
            history: self.read_history(),
            saved: self.read_saved(),
            notes: self.read_notes(),
        };
        self.shared.publish(fresh);
    }

    fn refresh_history(&mut self) {
        let history = self.read_history();
        self.shared.publish(Fresh {
            history,
            ..Fresh::default()
        });
    }

    fn read_history(&self) -> Option<Vec<crate::store::Stored>> {
        let Some(store) = self.store.as_ref() else {
            return Some(Vec::new());
        };
        match store.history(self.limits.max_entries) {
            Ok(history) => Some(history),
            Err(_) => {
                self.shared.fail(FailKind::Store);
                None
            }
        }
    }

    pub(super) fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Captured(captured) => self.capture(captured),
            Msg::Copy(id, reply) => {
                let written = self.copy(id);
                match reply {
                    Some(reply) => self.answer(&reply, written),
                    None => self.record(written),
                }
            }
            Msg::Delete(id) => {
                let changed = self.change(|store| store.delete(id));
                self.record(changed);
            }
            Msg::Clear(reply) => {
                let cleared = self.change(|store| store.clear_history());
                self.answer(&reply, cleared);
            }
            Msg::Wipe(reply) => {
                let wiped = self.wipe();
                self.answer(&reply, wiped);
            }
            Msg::Saved(msg) => self.handle_saved(msg),
            Msg::Note(msg) => self.handle_note(msg),
            #[cfg(test)]
            Msg::Wake => {}
        }
    }

    fn capture(&mut self, captured: Captured) {
        if !self.shared.is_recording() {
            return;
        }
        if let Ok(entry) = admit(captured, &self.limits) {
            let recorded = self.change(|store| store.record(&entry));
            self.record(recorded);
        }
    }

    fn copy(&mut self, id: i64) -> Result<(), FailKind> {
        let store = self.open_store().ok_or(FailKind::Store)?;
        let row = match store.get(id) {
            Ok(Some(row)) => row,
            Ok(None) => return Err(FailKind::NotFound),
            Err(_) => return Err(FailKind::Store),
        };
        let sink = &self.ports.sink;
        let written = match &row.entry.content {
            Content::Text(text) => sink.write_text(text),
            Content::Paths { paths, transfer } => {
                sink.write_files(paths, *transfer == Transfer::Cut)
            }
        };
        written.map_err(|_| FailKind::Sink)
    }

    fn wipe(&mut self) -> Result<(), FailKind> {
        self.open_store().ok_or(FailKind::Store)?;
        let store = self.store.take().ok_or(FailKind::Store)?;
        let wiped = match store.wipe_all(&self.path) {
            Ok(fresh) => {
                self.store = Some(fresh);
                Ok(())
            }
            Err(_) => {
                self.store = ClipStore::open(&self.path, self.limits).ok();
                Err(FailKind::Store)
            }
        };
        self.refresh();
        wiped
    }

    /// Runs `op`, reads the history again and returns the outcome; the caller records it.
    fn change(
        &mut self,
        op: impl FnOnce(&mut ClipStore) -> Result<(), ClipError>,
    ) -> Result<(), FailKind> {
        self.open_store().ok_or(FailKind::Store)?;
        let store = self.store.as_mut().ok_or(FailKind::Store)?;
        let changed = op(store).map_err(|_| FailKind::Store);
        self.refresh_history();
        changed
    }

    /// The store, opened again first when an earlier wipe lost it.
    pub(super) fn open_store(&mut self) -> Option<&ClipStore> {
        if self.store.is_none() {
            match ClipStore::open(&self.path, self.limits) {
                Ok(store) => {
                    self.store = Some(store);
                    self.refresh();
                }
                Err(_) => self.shared.fail(FailKind::Store),
            }
        }
        self.store.as_ref()
    }

    fn record(&self, result: Result<(), FailKind>) {
        if let Err(kind) = result {
            self.shared.fail(kind);
        }
    }

    /// Records the failure and sends the answer; a caller that stopped listening is fine.
    pub(super) fn answer<T>(
        &self,
        reply: &Sender<Result<T, FailKind>>,
        result: Result<T, FailKind>,
    ) {
        if let Err(kind) = &result {
            self.shared.fail(*kind);
        }
        let _ = reply.send(result);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(sequence) = self.timer.take() {
            self.clear_clipboard(sequence);
        }
    }
}

/// The failure a store error stands for.
pub(super) fn store_fail(error: &ClipError) -> FailKind {
    match error {
        ClipError::Missing | ClipError::NoteMissing => FailKind::NotFound,
        ClipError::NoteExists => FailKind::NoteExists,
        ClipError::TooLarge { .. } => FailKind::TooLarge,
        ClipError::TooManySaved { .. } => FailKind::Full,
        ClipError::Malformed(_) | ClipError::PlainSecret | ClipError::SecretNotSealed => {
            FailKind::Refused
        }
        _ => FailKind::Store,
    }
}

#[cfg(test)]
mod tests;
