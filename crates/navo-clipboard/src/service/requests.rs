//! Requests for saved items, secrets and notes. Each runs on the worker; results show in the
//! next snapshot and in the reply.

use std::sync::mpsc::{Sender, channel};

use crate::model::{Saved, Scope};
use crate::store::NoteListed;

use super::notes::NoteMsg;
use super::saved::SavedMsg;
use super::secret::SecretBytes;
use super::worker::Msg;
use super::{ClipService, FailKind, How, Reply};

impl ClipService {
    /// Saves a new item and replies with its id; `item.id` and `item.sealed` are ignored.
    /// A secret needs `secret` and no body: the worker seals it and the store only sees the
    /// sealed bytes. Without an OS sealer a secret fails with `SecretsUnsupported`.
    pub fn save(&self, item: Saved, secret: Option<SecretBytes>) -> Reply<i64> {
        self.request(|reply| {
            Msg::Saved(SavedMsg::Save {
                item,
                secret,
                reply,
            })
        })
    }

    /// Replaces the item with `item.id`. A secret without `secret` keeps its sealed text.
    /// A secret becomes another kind only when `item.body` holds the new non-empty text and
    /// `secret` is `None`; that update replaces the sealed bytes for good. Any other update of
    /// a secret into a plain kind is refused and the secret stays as it was.
    pub fn update(&self, item: Saved, secret: Option<SecretBytes>) -> Reply<()> {
        self.request(|reply| {
            Msg::Saved(SavedMsg::Update {
                item,
                secret,
                reply,
            })
        })
    }

    /// Deletes a saved item, also one the snapshot counts as skipped.
    pub fn delete_saved(&self, id: i64) -> bool {
        self.send(Msg::Saved(SavedMsg::Delete(id)))
    }

    /// Saves a copy of a history row as a global snippet, or a path item for a file list.
    /// The history row stays.
    pub fn save_from_history(&self, id: i64, at_ms: i64) -> Reply<i64> {
        self.request(|reply| Msg::Saved(SavedMsg::FromHistory { id, at_ms, reply }))
    }

    /// Writes a history row like `copy` and replies once the sink has written it or failed.
    pub fn copy_answered(&self, id: i64, _how: How) -> Reply<()> {
        self.request(|reply| Msg::Copy(id, Some(reply)))
    }

    /// Writes a saved item to the clipboard. A secret goes as a private copy that is emptied
    /// after `SECRET_CLEAR_AFTER`, or at `stop` if sooner, unless something newer was copied.
    pub fn copy_saved(&self, id: i64, _how: How) -> Reply<()> {
        self.request(|reply| Msg::Saved(SavedMsg::Copy { id, reply }))
    }

    /// Writes the user name of a secret as a private copy, cleared like the secret itself.
    /// An item that is no secret or has no user name is refused.
    pub fn copy_user(&self, id: i64) -> Reply<()> {
        self.request(|reply| Msg::Saved(SavedMsg::CopyUser { id, reply }))
    }

    /// Unseals a secret on the worker.
    pub fn reveal(&self, id: i64) -> Reply<SecretBytes> {
        self.request(|reply| Msg::Saved(SavedMsg::Reveal { id, reply }))
    }

    /// The body of the note for `scope`.
    pub fn note(&self, scope: Scope) -> Reply<Option<String>> {
        self.request(|reply| Msg::Note(NoteMsg::Get { scope, reply }))
    }

    /// Sets the note for `scope`; an empty body deletes it.
    pub fn set_note(&self, scope: Scope, body: String, at_ms: i64) -> Reply<()> {
        self.request(|reply| {
            Msg::Note(NoteMsg::Set {
                scope,
                body,
                at_ms,
                reply,
            })
        })
    }

    /// Every readable note with its body.
    pub fn notes(&self) -> Reply<NoteListed> {
        self.request(|reply| Msg::Note(NoteMsg::List(reply)))
    }

    /// Points the note of folder `from` at folder `to`.
    pub fn move_note(&self, from: Scope, to: Scope, at_ms: i64) -> Reply<()> {
        self.request(|reply| {
            Msg::Note(NoteMsg::Move {
                from,
                to,
                at_ms,
                reply,
            })
        })
    }

    /// Removes the note rows the snapshot counts as skipped; replies how many.
    pub fn delete_unreadable_notes(&self) -> Reply<usize> {
        self.request(|reply| Msg::Note(NoteMsg::DeleteUnreadable(reply)))
    }

    /// Queues the message; a dropped one answers `Busy` at once.
    pub(super) fn request<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, FailKind>>) -> Msg,
    ) -> Reply<T> {
        let (reply, answer) = channel();
        let fallback = reply.clone();
        if !self.send(make(reply)) {
            let _ = fallback.send(Err(FailKind::Busy));
        }
        answer
    }
}
