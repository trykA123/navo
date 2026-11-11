//! Text of one diff line: the cell, the changed-word runs and their tint.
//! Part of the file diff body; `body.rs` places these cells in each row.

use std::ops::Range;

use gpui_kit::{HighlightStyle, StyledText};

use super::{BodyLook, Side};
use crate::compare_diff::rows::ShownKind;
use crate::prelude::*;
use crate::tokens::Theme;

/// The text of one line, with `runs` (byte ranges) tinted; it never wraps.
pub(super) fn line_text(text: Option<&str>, runs: Vec<Range<usize>>, tint: Hsla) -> Div {
    let cell = div()
        .flex_1()
        .min_w_0()
        .whitespace_nowrap()
        .overflow_hidden();
    let Some(text) = text else {
        return cell;
    };
    if runs.is_empty() {
        return cell.child(SharedString::from(text.to_owned()));
    }
    let style = HighlightStyle {
        background_color: Some(tint),
        ..HighlightStyle::default()
    };
    let runs = runs.into_iter().map(|run| (run, style));
    cell.child(StyledText::new(text.to_owned()).with_highlights(runs))
}

/// The changed byte ranges of line `line` on `side` that fall on character boundaries of `text`,
/// ordered and apart, or none outside the Rich look. A bad range is skipped, never a panic.
pub(super) fn span_runs(
    look: &BodyLook,
    side: Side,
    line: Option<(u32, &str)>,
) -> Vec<Range<usize>> {
    let (Some(spans), Some((at, text))) = (look.spans, line) else {
        return Vec::new();
    };
    let found = match side {
        Side::Left => spans.left(at),
        Side::Right => spans.right(at),
    };
    let mut floor = 0;
    let mut runs = Vec::new();
    for range in found {
        let (start, end) = (range.start as usize, range.end as usize);
        let fits = floor <= start && start < end && end <= text.len();
        if fits && text.is_char_boundary(start) && text.is_char_boundary(end) {
            runs.push(start..end);
            floor = end;
        }
    }
    runs
}

/// The tint of changed words in a row of `kind`: the stronger step of the row's own tone.
pub(super) fn span_tint(t: &Theme, kind: ShownKind) -> Hsla {
    match kind {
        ShownKind::Deleted => t.diff.delete.fill,
        ShownKind::Inserted => t.diff.add.fill,
        _ => t.diff.change.fill,
    }
}
