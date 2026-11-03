//! The preview pane of a selected note: the editor, and a calm line that says it saves as you type.

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::Editor;
use gpui_kit::{
    InteractiveElement, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, div,
};
use navo_l10n::{t, tr};

use super::age::age;
use super::notes::is_placeholder;
use super::preview::{self, Look};
use super::rows::RowElement;
use super::view::ClipPalette;
use super::{ClipKind, ClipRow};
use crate::tokens::Theme;

impl ClipPalette {
    fn saved_line(&self, row: &ClipRow) -> String {
        if let Some(reason) = self.note.failure() {
            return t!("clip-note-not-saved", reason = reason.to_owned());
        }
        if self.note.is_dirty() || is_placeholder(row) || row.when_ms <= 0 {
            return tr("clip-note-saved");
        }
        t!("clip-note-saved-age", age = age(self.now_ms, row.when_ms))
    }

    fn note_status(&self, t: &Theme, row: &ClipRow) -> RowElement {
        let text = SharedString::from(self.saved_line(row));
        div()
            .id("clip-palette-note-saved")
            .role(Role::Status)
            .aria_label(text.clone())
            .test_support()
            .flex_none()
            .truncate()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(text)
    }

    fn note_field(&self, t: &Theme, is_focused: bool) -> RowElement {
        let editor = Editor::new(&self.note.editor)
            .h_full()
            .appearance(false)
            .bordered(false)
            .aria_label(tr("clip-note-label"))
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body);
        div()
            .id("clip-palette-note")
            .test_support()
            .flex_1()
            .min_h_0()
            .p(t.spacing.sm)
            .rounded(t.radii.md)
            .border_1()
            .border_color(if is_focused {
                t.colors.accent
            } else {
                t.colors.line
            })
            .bg(t.surfaces.field)
            .overflow_hidden()
            .child(editor)
    }

    /// The pane for a note row once the editor holds that note; none until then.
    pub(super) fn note_pane(&self, t: &Theme, row: &ClipRow, look: Look<'_>) -> Option<RowElement> {
        if row.kind != ClipKind::Note || !self.note.holds(&row.scope) {
            return None;
        }
        let body = if self.note.is_readable() {
            self.note_field(t, look.has_note_focus)
        } else {
            div()
                .id("clip-palette-note-unreadable")
                .role(Role::Status)
                .test_support()
                .flex_1()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(tr("clip-note-unreadable"))
        };
        let pane = preview::frame(t, look.is_docked)
            .child(preview::head(t, row))
            .child(body)
            .child(self.note_status(t, row));
        Some(if look.is_docked {
            pane
        } else {
            pane.child(preview::details(t, row, look.now_ms))
        })
    }
}
