//! The Stream's details drawer (Ctrl+I, D58): the current file's path, status, what its card
//! holds and its Viewed mark. It starts closed; Esc closes it before anything else.

use gpui_kit::Stateful;
use navo_l10n::{t, tr};

use super::super::drawers::{fact, frame};
use super::StreamScreen;
use crate::browser::status_text;
use crate::commands::compare::Details;
use crate::compare_diff::PairKind;
use crate::compare_diff::card::kind_text;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::NavoWindow;

/// The lines of the drawer for the current file, or the empty note with no file.
pub(super) fn lines(stream: &StreamScreen) -> Vec<String> {
    let model = &stream.model;
    let Some(item) = model.items().get(model.cursor().item) else {
        return vec![tr("compare-stream-empty")];
    };
    let card = model.card(item.id);
    let held = match card {
        None => tr("compare-screen-comparing"),
        Some(card) if matches!(card.kind, PairKind::Text) && !card.hunks.is_empty() => t!(
            "compare-stream-details-changes",
            count = card.hunks.len(),
            added = card.added,
            removed = card.removed
        ),
        Some(card) => kind_text(Some(card.kind)),
    };
    let mut out = vec![item.path.clone(), status_text(item.status, None), held];
    if model.is_viewed(item.id) {
        out.push(tr("compare-card-viewed"));
    }
    out
}

/// The drawer on the right of the cards, or `None` while it is closed.
pub(super) fn drawer(t: &Theme, stream: &StreamScreen) -> Option<AnyElement> {
    if !stream.details {
        return None;
    }
    let lines = lines(stream).into_iter().map(|line| fact(t, line));
    let drawer = frame(t, "compare-stream-details", tr("compare-drawer-details"))
        .border_l_1()
        .children(lines);
    Some(drawer.into_any_element())
}

impl NavoWindow {
    /// Binds Ctrl+I (the details drawer) on the Stream's `root`.
    pub(super) fn stream_details_key(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &Details, _, cx| {
            if let Some(stream) = view.stream_mut() {
                stream.details = !stream.details;
                cx.notify();
            }
        }))
    }
}
