//! Folder notes on the worker thread.

use std::sync::mpsc::Sender;

use crate::model::Scope;
use crate::store::{ClipStore, NoteListed};

use super::FailKind;
use super::shared::Fresh;
use super::snapshot::NoteHeader;
use super::worker::{Worker, store_fail};

type Answer<T> = Sender<Result<T, FailKind>>;

pub(super) enum NoteMsg {
    Get {
        scope: Scope,
        reply: Answer<Option<String>>,
    },
    Set {
        scope: Scope,
        body: String,
        at_ms: i64,
        reply: Answer<()>,
    },
    List(Answer<NoteListed>),
    Move {
        from: Scope,
        to: Scope,
        at_ms: i64,
        reply: Answer<()>,
    },
    DeleteUnreadable(Answer<usize>),
}

impl Worker {
    pub(super) fn handle_note(&mut self, msg: NoteMsg) {
        match msg {
            NoteMsg::Get { scope, reply } => {
                let result = self.read_store(|store| store.note(&scope));
                self.answer(&reply, result);
            }
            NoteMsg::Set {
                scope,
                body,
                at_ms,
                reply,
            } => {
                let result = self.change_notes(|store| store.set_note(&scope, &body, at_ms));
                self.answer(&reply, result);
            }
            NoteMsg::List(reply) => {
                let result = self.read_store(ClipStore::notes);
                self.answer(&reply, result);
            }
            NoteMsg::Move {
                from,
                to,
                at_ms,
                reply,
            } => {
                let result = self.change_notes(|store| store.move_note(&from, &to, at_ms));
                self.answer(&reply, result);
            }
            NoteMsg::DeleteUnreadable(reply) => {
                let result = self.change_notes(ClipStore::delete_unreadable_notes);
                self.answer(&reply, result);
            }
        }
    }

    fn read_store<T>(
        &mut self,
        op: impl FnOnce(&ClipStore) -> Result<T, crate::store::ClipError>,
    ) -> Result<T, FailKind> {
        let store = self.open_store().ok_or(FailKind::Store)?;
        op(store).map_err(|error| store_fail(&error))
    }

    /// Runs `op` and publishes the note list again, whatever the result.
    fn change_notes<T>(
        &mut self,
        op: impl FnOnce(&ClipStore) -> Result<T, crate::store::ClipError>,
    ) -> Result<T, FailKind> {
        let result = self.read_store(op);
        let notes = self.read_notes();
        self.shared.publish(Fresh {
            notes,
            ..Fresh::default()
        });
        result
    }

    pub(super) fn read_notes(&self) -> Option<(Vec<NoteHeader>, usize)> {
        let listed = self.store.as_ref().map(ClipStore::notes).transpose();
        match listed {
            Ok(Some(listed)) => {
                let headers = listed.notes.into_iter().map(|note| NoteHeader {
                    scope: note.scope,
                    updated_ms: note.updated_ms,
                });
                Some((headers.collect(), listed.skipped))
            }
            Ok(None) => Some((Vec::new(), 0)),
            Err(_) => {
                self.shared.fail(FailKind::Store);
                None
            }
        }
    }
}
