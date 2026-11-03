//! Editing saved items and re-pointing notes behind the hooks.
//!
//! A secret's password never reaches the UI here. An update of a secret with an empty body
//! sends no secret text, and the worker reuses the sealed bytes it already holds.

use navo_clipboard::{FailKind, Saved, SavedKind};
use navo_ui::clip_palette::{ClipId, ClipKind, SavedDraft, SavedEdit, Scope, SecretText};

use super::ClipModule;
use super::rows::{self, Target, location};
use super::saved::{item_of, message, now_ms, scope_of, wait};

fn refused() -> String {
    message(FailKind::Refused)
}

/// The edit form's kind for a stored one; a folder-note row has no form.
fn kind_of(kind: SavedKind) -> Option<ClipKind> {
    match kind {
        SavedKind::Snippet | SavedKind::Command => Some(ClipKind::Text),
        SavedKind::Path => Some(ClipKind::Path),
        SavedKind::Secret => Some(ClipKind::Secret),
        SavedKind::Note => None,
    }
}

impl ClipModule {
    /// The saved item from the snapshot, which holds no sealed bytes.
    fn saved_item(
        &self,
        id: ClipId,
    ) -> Result<(std::sync::Arc<navo_clipboard::ClipService>, Saved), String> {
        let service = self.running()?;
        let Some(Target::Saved(row)) = rows::target(id) else {
            return Err(refused());
        };
        let found = service
            .snapshot()
            .saved
            .iter()
            .find(|item| item.id == row)
            .cloned();
        let item = found.ok_or_else(|| message(FailKind::NotFound))?;
        Ok((service, item))
    }

    /// Reads from the snapshot, so it never waits. A secret comes back with an empty body.
    /// A folder that is not a valid location refuses, rather than widening the scope.
    pub(super) fn load_for_edit(&self, id: ClipId) -> Result<SavedEdit, String> {
        let (_, item) = self.saved_item(id)?;
        let kind = kind_of(item.kind).ok_or_else(refused)?;
        let scope = match &item.scope {
            navo_clipboard::Scope::Global => Scope::Everywhere,
            navo_clipboard::Scope::Folder(path) => {
                Scope::Folder(location(path).ok_or_else(refused)?)
            }
        };
        let body = match item.kind {
            SavedKind::Secret => String::new(),
            _ => item.body.unwrap_or_default(),
        };
        Ok(SavedEdit {
            kind,
            title: item.title,
            body: SecretText::new(body),
            user: item.user,
            scope,
        })
    }

    /// A text edit keeps a command a command. A secret draft with an empty body sends no secret
    /// text, so the worker keeps the sealed password; the worker also refuses a kind change
    /// that has no new non-empty body.
    pub(super) fn update_from(&self, id: ClipId, draft: &SavedDraft) -> Result<(), String> {
        let (service, existing) = self.saved_item(id)?;
        let (mut item, mut secret) = item_of(draft)?;
        if draft.body.expose().is_empty() {
            secret = None;
        }
        if secret.is_some() && !self.backends.secrets {
            return Err(message(FailKind::SecretsUnsupported));
        }
        if kind_of(existing.kind) == kind_of(item.kind) {
            item.kind = existing.kind;
        }
        item.id = existing.id;
        item.created_ms = existing.created_ms;
        wait(service.update(item, secret))
    }

    pub(super) fn move_note_to(&self, from: &Scope, to: &Scope) -> Result<(), String> {
        let service = self.running()?;
        let (from, to) = (scope_of(from)?, scope_of(to)?);
        wait(service.move_note(from, to, now_ms()))
    }
}
