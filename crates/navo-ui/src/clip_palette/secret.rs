//! Showing a secret in the preview. The text lives in one wiped buffer, only while its row
//! stays selected.

use gpui_kit::Context;

use super::actions::{Action, items};
use super::view::{ClipPalette, ClipPaletteEvent, Place};
use super::{ClipId, ClipRow, SecretText};

/// The shown secret. No `Debug` or `Clone`: dropping it wipes the only copy.
pub(super) struct Revealed {
    id: ClipId,
    text: SecretText,
}

/// What decides which items a row offers.
#[derive(Clone, Copy, Default)]
pub(super) struct Facts {
    pub is_shown: bool,
    pub has_user: bool,
    pub place: Place,
}

impl ClipPalette {
    pub(super) fn facts(&self, row: &ClipRow) -> Facts {
        Facts {
            is_shown: self.shown_secret(row).is_some(),
            has_user: self.state.has_user(row.id),
            place: self.place,
        }
    }

    /// The text of `row` while it is shown.
    pub(super) fn shown_secret(&self, row: &ClipRow) -> Option<&str> {
        self.overlay
            .revealed
            .as_ref()
            .filter(|shown| shown.id == row.id)
            .map(|shown| shown.text.expose())
    }

    /// Shows the selected secret, or hides it again; nothing for any other row.
    pub fn toggle_reveal(&mut self, cx: &mut Context<Self>) {
        self.run_if_offered(Action::Reveal, cx);
    }

    pub(super) fn copy_user_key(&mut self, cx: &mut Context<Self>) {
        self.run_if_offered(Action::CopyUser, cx);
    }

    pub(super) fn save_key(&mut self, cx: &mut Context<Self>) {
        self.run_if_offered(Action::Save, cx);
    }

    /// Runs an action from a key, unless the selected row does not offer it.
    pub(super) fn run_if_offered(&mut self, action: Action, cx: &mut Context<Self>) {
        let is_offered = self.state.selected_row().is_some_and(|row| {
            items(row, self.facts(row))
                .iter()
                .any(|i| i.action == action)
        });
        if is_offered {
            self.run(action, cx);
        }
    }

    pub(super) fn flip_reveal(&mut self, id: ClipId) -> Result<(), String> {
        if self.overlay.revealed.take().is_none() {
            let text = self.hooks.reveal(id)?;
            self.overlay.revealed = Some(Revealed { id, text });
        }
        Ok(())
    }

    /// Hides the secret once the selection has left its row.
    pub(super) fn hide_stray_secret(&mut self) {
        let selected = self.state.selected_row().map(|row| row.id);
        if self.overlay.revealed.as_ref().map(|shown| shown.id) != selected {
            self.overlay.revealed = None;
        }
    }

    /// Hides any shown secret and tells the host the palette is done.
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.overlay.revealed = None;
        cx.emit(ClipPaletteEvent::Dismissed);
    }
}
