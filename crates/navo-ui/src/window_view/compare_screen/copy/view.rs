//! What the copy draws (V27, V32): the bars on the file diff's rows and the status line's note.

use std::rc::Rc;

use gpui_kit::WeakEntity;
use gpui_kit::base::TestSupportExt;
use navo_l10n::tr;

use super::super::pick::PickDraw;
use super::Hover;
use crate::compare_diff::copy_bar::CardBars;
use crate::compare_diff::copy_bar::{BarRow, hunk_row};
use crate::file_list::UndoLast;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;

/// What the status line says about the last copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::compare_screen) struct Note {
    pub(in crate::window_view::compare_screen) text: SharedString,
    /// Whether an Undo button follows the text.
    pub(in crate::window_view::compare_screen) undo: bool,
    pub(in crate::window_view::compare_screen) error: bool,
}

/// What the file diff's list needs to draw the bars: the window and the hovered change.
#[derive(Clone)]
pub(in crate::window_view::compare_screen) struct FileBars {
    pub(in crate::window_view::compare_screen) weak: WeakEntity<NavoWindow>,
    pub(in crate::window_view::compare_screen) hovered: Option<u32>,
    pub(in crate::window_view::compare_screen) animate: bool,
    /// The tick boxes of the picked lines.
    pub(in crate::window_view::compare_screen) picks: PickDraw,
}

impl FileBars {
    /// Wraps the drawn `row` (number `at`, first row of its change when `first`) of change
    /// `hunk` so the pointer shows the bar and its buttons copy.
    pub(in crate::window_view::compare_screen) fn wrap(
        &self,
        t: &Theme,
        row: AnyElement,
        (at, hunk, first): (usize, u32, bool),
    ) -> AnyElement {
        let (on_hover, on_copy) = (self.weak.clone(), self.weak.clone());
        let bar = BarRow {
            id: ("compare-file-row", at),
            first,
            hovered: self.hovered == Some(hunk),
            animate: self.animate,
            on_hover: Rc::new(move |on, cx| {
                let _ = on_hover.update(cx, |view, cx| {
                    view.set_copy_hover(Hover::File(hunk), on, cx);
                });
            }),
            on_copy: Rc::new(move |direction, _, cx| {
                let _ = on_copy.update(cx, |view, cx| {
                    view.copy_change_to(direction, Some(Hover::File(hunk)), cx);
                });
            }),
        };
        hunk_row(t, row, bar)
    }
}

/// The status line's copy note: the text and, after a copy, an Undo button that runs
/// `files.undo` like Ctrl+Z.
pub(in crate::window_view::compare_screen) fn note_view(
    t: &Theme,
    note: &Note,
) -> impl IntoElement {
    let color = if note.error {
        t.colors.warn
    } else {
        t.colors.text
    };
    div()
        .id("compare-copy-note")
        .role(gpui_kit::Role::Status)
        .aria_label(note.text.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .text_size(t.typography.small)
        .text_color(color)
        .child(note.text.clone())
        .when(note.undo, |row| {
            row.child(
                Button::new("compare-copy-undo", tr("compare-copy-undo"))
                    .variant(ButtonVariant::Ghost)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(UndoLast), cx)),
            )
        })
}

impl NavoWindow {
    /// The bars of the Stream's cards: the hovered change and what the buttons call.
    pub(in crate::window_view::compare_screen) fn card_bars(
        &self,
        weak: &WeakEntity<NavoWindow>,
        cx: &App,
    ) -> CardBars {
        let hovered = match self.compare.diff.copy.hover {
            Some(Hover::Card { item, hunk }) => Some((item, hunk)),
            _ => None,
        };
        let (on_hover, on_copy) = (weak.clone(), weak.clone());
        CardBars {
            hovered,
            animate: !cx.reduce_motion(),
            on_hover: Rc::new(move |item, hunk, on, cx| {
                let _ = on_hover.update(cx, |view, cx| {
                    view.set_copy_hover(Hover::Card { item, hunk }, on, cx);
                });
            }),
            on_copy: Rc::new(move |item, hunk, direction, _, cx| {
                let _ = on_copy.update(cx, |view, cx| {
                    view.copy_change_to(direction, Some(Hover::Card { item, hunk }), cx);
                });
            }),
        }
    }
}
