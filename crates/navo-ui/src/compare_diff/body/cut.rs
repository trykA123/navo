//! The column cut of one file diff row (D39.2, D39.5): the columns its text shows, how far the
//! text moves left, whether the row continues a wrapped line, and the probe that measures it.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{HighlightStyle, canvas, combine_highlights};

use super::syntax::syntax_runs;
use super::text::span_tint;
use super::{BodyLook, Side, span_runs};
use crate::compare_diff::cols::{Shown, cut};
use crate::compare_diff::long::is_long;
use crate::compare_diff::rows::{ShownKind, ShownRow};
use crate::prelude::*;

/// The bounds of the text cells seen in the last layout. Unified rows write `left` only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProbeSeen {
    pub left: Bounds<Pixels>,
    pub right: Bounds<Pixels>,
}

/// Shared with the view that owns the sideways state; the row's canvas writes it each layout.
pub type TextProbe = Rc<Cell<ProbeSeen>>;

/// How one row cuts its text. The default draws whole lines, as before the cut existed.
#[derive(Clone, Default)]
pub struct RowCut {
    /// The display columns to draw; `None` draws the whole line.
    pub cols: Option<Range<u32>>,
    /// How far the text moves left inside its cell, in px (the part of a column cut away).
    pub nudge: Pixels,
    /// A later visual line of a wrapped row: no number, tick or "Show bytes here".
    pub continued: bool,
    /// Where the text cells report their bounds; `None` measures nothing.
    pub probe: Option<TextProbe>,
}

/// The row whose number, tick and long button are drawn: `row` itself, or one with no lines
/// when `cut` continues a wrapped line, so those cells stay empty but keep their width.
pub(super) fn lead_row<'a>(row: &ShownRow<'a>, cut: &RowCut) -> ShownRow<'a> {
    if cut.continued {
        ShownRow {
            left: None,
            right: None,
            ..*row
        }
    } else {
        *row
    }
}

/// One line to draw: the kind of its row, the side and line it shows, and the probe slot its
/// cell reports to (unified rows report to `left`).
pub(super) struct LineAt<'a> {
    pub kind: ShownKind,
    pub side: Side,
    pub line: Option<(u32, &'a str)>,
    pub slot: Side,
}

/// The text and runs of `at`'s line after the cut, or `None` for no line: syntax colours with
/// the changed-word fill over them. A line over
/// `LONG_LINE` is never cut: it draws whole and keeps its "Show bytes here" button.
pub(super) fn shown_line(
    look: &BodyLook,
    at: &LineAt,
    row_cut: &RowCut,
) -> Option<Shown<HighlightStyle>> {
    let (_, text) = at.line?;
    let style = HighlightStyle {
        background_color: Some(span_tint(look.theme, at.kind)),
        ..HighlightStyle::default()
    };
    let fills = span_runs(look, at.side, at.line)
        .into_iter()
        .map(|run| (run, style));
    let colours = syntax_runs(look, at);
    let runs: Vec<_> = if colours.is_empty() {
        fills.collect()
    } else {
        combine_highlights(colours, fills).collect()
    };
    match &row_cut.cols {
        Some(cols) if !is_long(text) => Some(cut(text, &runs, cols.clone())),
        _ => Some(Shown {
            text: text.to_owned(),
            runs,
        }),
    }
}

/// A canvas over the whole text cell that stores its bounds in `slot` of `probe`, and asks for
/// one more frame when the width changed, so the cut can follow a resize.
pub(super) fn probe_canvas(probe: &TextProbe, slot: Side) -> impl IntoElement {
    let probe = probe.clone();
    canvas(
        move |bounds, window, _| {
            let mut seen = probe.get();
            let at = match slot {
                Side::Left => &mut seen.left,
                Side::Right => &mut seen.right,
            };
            let widened = at.size.width != bounds.size.width;
            *at = bounds;
            probe.set(seen);
            if widened {
                window.request_animation_frame();
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod tests;
