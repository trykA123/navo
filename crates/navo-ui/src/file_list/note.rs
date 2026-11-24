//! Row notes: a short muted text after a row's name, such as why an archive entry is unsafe.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div,
};

use super::FileList;
use crate::tokens::Theme;

/// Turns a row's name into its note; None means the row has no note.
pub type NoteFn = fn(&str) -> Option<SharedString>;

impl FileList {
    /// Shows `note(name)` as muted text after each row's name; None shows no notes.
    #[must_use]
    pub(crate) fn note_of(mut self, note: Option<NoteFn>) -> Self {
        self.note_of = note;
        self
    }
}

/// The muted note after a row's name; it truncates before the name does.
pub(super) fn note(t: &Theme, text: &SharedString) -> impl IntoElement {
    div()
        .id("row-note")
        .aria_label(text.clone())
        .test_support()
        .min_w_0()
        .truncate()
        .text_color(t.colors.muted)
        .child(text.clone())
}
