//! Saved items and secrets on the worker thread.

use std::sync::mpsc::Sender;

use crate::model::{Content, Saved, SavedKind, Scope, Sealed};
use crate::search::join_paths;

use super::FailKind;
use super::secret::{SealError, SecretBytes, Sequence, Settled};
use super::shared::Fresh;
use super::worker::{Worker, store_fail};

const TITLE_CHARS: usize = 80;

type Answer<T> = Sender<Result<T, FailKind>>;

pub(super) enum SavedMsg {
    Save {
        item: Saved,
        secret: Option<SecretBytes>,
        reply: Answer<i64>,
    },
    Update {
        item: Saved,
        secret: Option<SecretBytes>,
        reply: Answer<()>,
    },
    Delete {
        id: i64,
        reply: Answer<()>,
    },
    FromHistory {
        id: i64,
        at_ms: i64,
        reply: Answer<i64>,
    },
    Copy {
        id: i64,
        reply: Answer<()>,
    },
    Reveal {
        id: i64,
        reply: Answer<SecretBytes>,
    },
    CopyUser {
        id: i64,
        reply: Answer<()>,
    },
}

fn seal_fail(error: SealError) -> FailKind {
    match error {
        SealError::Unsupported => FailKind::SecretsUnsupported,
        SealError::Failed => FailKind::Seal,
    }
}

/// The item a snapshot may hold: a secret loses its sealed bytes.
fn shown(mut item: Saved) -> Saved {
    item.sealed = None;
    item
}

/// The first non-blank line, trimmed, at most `TITLE_CHARS` characters.
fn title_of(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty());
    line.unwrap_or_default().chars().take(TITLE_CHARS).collect()
}

impl Worker {
    pub(super) fn handle_saved(&mut self, msg: SavedMsg) {
        match msg {
            SavedMsg::Save {
                item,
                secret,
                reply,
            } => {
                let result = self.save(item, secret);
                self.answer(&reply, result);
            }
            SavedMsg::Update {
                item,
                secret,
                reply,
            } => {
                let result = self.update(item, secret);
                self.answer(&reply, result);
            }
            SavedMsg::Delete { id, reply } => {
                let result =
                    self.with_store(|store| store.delete_saved(id).map_err(|e| store_fail(&e)));
                self.answer(&reply, result);
            }
            SavedMsg::FromHistory { id, at_ms, reply } => {
                let result = self.save_from_history(id, at_ms);
                self.answer(&reply, result);
            }
            SavedMsg::Copy { id, reply } => {
                let result = self.copy_saved(id);
                self.answer(&reply, result);
            }
            SavedMsg::Reveal { id, reply } => {
                let result = self.reveal(id);
                self.answer(&reply, result);
            }
            SavedMsg::CopyUser { id, reply } => {
                let result = self.copy_user(id);
                self.answer(&reply, result);
            }
        }
    }

    /// Runs `op` on the store and publishes the saved list again, whatever the result.
    fn with_store<T>(
        &mut self,
        op: impl FnOnce(&crate::store::ClipStore) -> Result<T, FailKind>,
    ) -> Result<T, FailKind> {
        let result = self.open_store().ok_or(FailKind::Store).and_then(op);
        self.refresh_saved();
        result
    }

    pub(super) fn read_saved(&self) -> Option<(Vec<Saved>, usize)> {
        let listed = self.store.as_ref().map(|store| store.saved()).transpose();
        match listed {
            Ok(Some(listed)) => Some((
                listed.items.into_iter().map(shown).collect(),
                listed.skipped.len(),
            )),
            Ok(None) => Some((Vec::new(), 0)),
            Err(_) => {
                self.shared.fail(FailKind::Store);
                None
            }
        }
    }

    fn refresh_saved(&mut self) {
        let saved = self.read_saved();
        self.shared.publish(Fresh {
            saved,
            ..Fresh::default()
        });
    }

    /// Fills `item.sealed` from `secret` for a secret and clears it otherwise.
    fn seal_into(&self, item: &mut Saved, secret: Option<&SecretBytes>) -> Result<(), FailKind> {
        match (item.kind == SavedKind::Secret, secret) {
            (true, Some(secret)) => {
                let sealed = self
                    .ports
                    .sealer
                    .seal(secret.as_bytes())
                    .map_err(seal_fail)?;
                item.sealed = Some(Sealed(sealed));
                Ok(())
            }
            (false, None) => {
                item.sealed = None;
                Ok(())
            }
            _ => Err(FailKind::Refused),
        }
    }

    fn save(&mut self, mut item: Saved, secret: Option<SecretBytes>) -> Result<i64, FailKind> {
        if item.kind == SavedKind::Secret && item.body.is_some() {
            return Err(FailKind::Refused);
        }
        self.seal_into(&mut item, secret.as_ref())?;
        drop(secret);
        self.with_store(|store| store.save(&item).map_err(|e| store_fail(&e)))
    }

    /// Without new secret text, a secret keeps the sealed bytes it has. A secret turns into
    /// another kind only with a new non-empty body; otherwise the update is refused.
    fn update(&mut self, mut item: Saved, secret: Option<SecretBytes>) -> Result<(), FailKind> {
        if item.kind == SavedKind::Secret && item.body.is_some() {
            return Err(FailKind::Refused);
        }
        if item.kind != SavedKind::Secret && self.find_saved(item.id)?.sealed.is_some() {
            let has_text = item.body.as_ref().is_some_and(|body| !body.is_empty());
            if !has_text {
                return Err(FailKind::Refused);
            }
        }
        if item.kind == SavedKind::Secret && secret.is_none() {
            item.sealed = Some(self.sealed_of(item.id)?);
        } else {
            self.seal_into(&mut item, secret.as_ref())?;
        }
        drop(secret);
        self.with_store(|store| store.update(&item).map_err(|e| store_fail(&e)))
    }

    fn find_saved(&mut self, id: i64) -> Result<Saved, FailKind> {
        let store = self.open_store().ok_or(FailKind::Store)?;
        let listed = store.saved().map_err(|e| store_fail(&e))?;
        let found = listed.items.into_iter().find(|item| item.id == id);
        found.ok_or(FailKind::NotFound)
    }

    fn sealed_of(&mut self, id: i64) -> Result<Sealed, FailKind> {
        let item = self.find_saved(id)?;
        item.sealed.ok_or(FailKind::Refused)
    }

    fn reveal(&mut self, id: i64) -> Result<SecretBytes, FailKind> {
        let sealed = self.sealed_of(id)?;
        self.ports.sealer.unseal(&sealed.0).map_err(seal_fail)
    }

    fn save_from_history(&mut self, id: i64, at_ms: i64) -> Result<i64, FailKind> {
        let store = self.open_store().ok_or(FailKind::Store)?;
        let row = store.get(id).map_err(|e| store_fail(&e))?;
        let row = row.ok_or(FailKind::NotFound)?;
        let (kind, title, body) = match row.entry.content {
            Content::Text(text) => (SavedKind::Snippet, title_of(&text), text),
            Content::Paths { paths, .. } => {
                let body = join_paths(&paths);
                (SavedKind::Path, title_of(&body), body)
            }
        };
        let item = Saved {
            id: 0,
            kind,
            title,
            body: Some(body),
            user: None,
            sealed: None,
            scope: Scope::Global,
            created_ms: at_ms,
            updated_ms: at_ms,
        };
        self.with_store(|store| store.save(&item).map_err(|e| store_fail(&e)))
    }

    /// A secret goes out as a private copy and is cleared later; any other item as text.
    fn copy_saved(&mut self, id: i64) -> Result<(), FailKind> {
        let item = self.find_saved(id)?;
        let Some(sealed) = item.sealed else {
            let body = item.body.unwrap_or_default();
            return self
                .ports
                .sink
                .write_text(&body)
                .map_err(|_| FailKind::Sink);
        };
        let plain = self.ports.sealer.unseal(&sealed.0).map_err(seal_fail)?;
        let text = plain.as_str().ok_or(FailKind::Seal)?;
        self.private_copy(text)
    }

    /// The user name of a secret as a private copy, cleared like the secret itself.
    fn copy_user(&mut self, id: i64) -> Result<(), FailKind> {
        let item = self.find_saved(id)?;
        match item.user {
            Some(user) if item.kind == SavedKind::Secret && !user.is_empty() => {
                self.private_copy(&user)
            }
            _ => Err(FailKind::Refused),
        }
    }

    fn private_copy(&mut self, text: &str) -> Result<(), FailKind> {
        self.clear_pending();
        let sequence = self
            .ports
            .sink
            .write_private(text)
            .map_err(|_| FailKind::Sink)?;
        self.timer.start(sequence, self.ports.clock.now());
        Ok(())
    }

    /// Tries to empty the clipboard of the pending secret copy; a failure keeps it pending.
    pub(super) fn clear_pending(&mut self) {
        let Some(sequence) = self.timer.sequence() else {
            return;
        };
        let done = self.ports.sink.clear_if_sequence(sequence).is_ok();
        if self.timer.settle(done, self.ports.clock.now()) == Settled::GaveUp {
            self.shared.fail(FailKind::Sink);
        }
    }

    /// Empties the clipboard when it still holds the secret copy with `sequence`.
    pub(super) fn clear_clipboard(&self, sequence: Sequence) {
        if self.ports.sink.clear_if_sequence(sequence).is_err() {
            self.shared.fail(FailKind::Sink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::title_of;

    #[test]
    fn a_title_is_the_first_non_blank_line_cut_to_80_characters() {
        assert_eq!(title_of("\n  \n  first line  \nsecond"), "first line");
        let long = "é".repeat(100);
        assert_eq!(title_of(&long).chars().count(), 80);
        assert_eq!(title_of(" \n "), "");
    }
}
