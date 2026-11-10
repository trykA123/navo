//! Draws the plan sheet (V31): the "Copy all N" buttons of the search row and the dialog over a
//! scrim while the sheet is open.

use navo_l10n::{t, tr};

use super::super::stream::StreamScreen;
use super::Phase;
use super::rows::props_of;
use crate::compare_diff::CopyDirection;
use crate::confirm_dialog::ConfirmDialog;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;

/// The "Copy all N" buttons for the search row, or `None` while nothing matches.
pub(in crate::window_view::compare_screen) fn buttons(
    t: &Theme,
    stream: &StreamScreen,
    cx: &mut Context<NavoWindow>,
) -> Option<Div> {
    let count = stream.model.match_counts().0;
    if count == 0 {
        return None;
    }
    let button = |id: &'static str, icon: IconName, tip: &str, direction: CopyDirection| {
        Button::new(id, t!("compare-sheet-button", count = count))
            .variant(ButtonVariant::Ghost)
            .icon(icon)
            .tooltip(tr(tip))
            .on_click(
                cx.listener(move |view, _: &ClickEvent, _, cx| view.sheet_open(direction, cx)),
            )
    };
    Some(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .child(button(
                "compare-copy-all-left",
                IconName::ArrowLeft,
                "compare-sheet-button-left",
                CopyDirection::ToLeft,
            ))
            .child(button(
                "compare-copy-all-right",
                IconName::ArrowRight,
                "compare-sheet-button-right",
                CopyDirection::ToRight,
            )),
    )
}

impl NavoWindow {
    /// The open sheet over a scrim, or `None` when none is asked.
    pub(in crate::window_view::compare_screen) fn sheet_view(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = &self.compare.diff.sheet;
        let Phase::Asking { direction, rows } = &sheet.phase else {
            return None;
        };
        let (confirm, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(("compare-sheet", sheet.asks), props_of(*direction, rows))
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.sheet_answer(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.sheet_answer(false, window, cx));
            });
        Some(
            div()
                .id("compare-sheet-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(tokens::current(cx).surfaces.scrim)
                .occlude()
                .child(dialog)
                .into_any_element(),
        )
    }
}
