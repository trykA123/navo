//! Text of one diff line: the cell, the changed-word runs and their tint.
//! Part of the file diff body; `body.rs` places these cells in each row.

use std::ops::Range;

use gpui_kit::StyledText;

use super::cut::{LineAt, probe_canvas, shown_line};
use super::{BodyLook, RowCut, Side};
use crate::compare_diff::cols::Shown;
use crate::compare_diff::rows::ShownKind;
use crate::prelude::*;
use crate::tokens::{DiffTone, Theme};

/// The text cell of `at`'s line: the columns `row_cut` keeps, changed words tinted, moved left
/// by its nudge; it never wraps. This is the one place that builds a line's styled text.
pub(super) fn line_text(look: &BodyLook, at: &LineAt, row_cut: &RowCut) -> Div {
    let cell = div()
        .relative()
        .flex_1()
        .min_w_0()
        .whitespace_nowrap()
        .overflow_hidden()
        .children(
            row_cut
                .probe
                .as_ref()
                .map(|probe| probe_canvas(probe, at.slot)),
        );
    let Some(Shown { text, runs }) = shown_line(look, at, row_cut) else {
        return cell;
    };
    let styled = if runs.is_empty() {
        SharedString::from(text).into_any_element()
    } else {
        StyledText::new(text)
            .with_highlights(runs)
            .into_any_element()
    };
    if row_cut.nudge > px(0.) {
        cell.child(div().ml(-row_cut.nudge).child(styled))
    } else {
        cell.child(styled)
    }
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

/// The tone of the text column of `kind` on `side`: a change on both sides, a deletion on the
/// left, an insertion on the right; `None` for equal, pending and filler cells.
pub(super) fn text_tone(t: &Theme, kind: ShownKind, side: Side) -> Option<DiffTone> {
    match (kind, side) {
        (ShownKind::Changed, _) => Some(t.diff.change),
        (ShownKind::Deleted, Side::Left) => Some(t.diff.delete),
        (ShownKind::Inserted, Side::Right) => Some(t.diff.add),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ShownKind, Side, text_tone};
    use crate::tokens::{Mode, Theme};

    #[test]
    fn text_tone_tints_changed_added_and_removed_text_only() {
        let t = Theme::new(Mode::Light);
        assert_eq!(
            text_tone(&t, ShownKind::Changed, Side::Left),
            Some(t.diff.change)
        );
        assert_eq!(
            text_tone(&t, ShownKind::Deleted, Side::Left),
            Some(t.diff.delete)
        );
        assert_eq!(
            text_tone(&t, ShownKind::Inserted, Side::Right),
            Some(t.diff.add)
        );
        assert_eq!(text_tone(&t, ShownKind::Deleted, Side::Right), None);
        assert_eq!(text_tone(&t, ShownKind::Inserted, Side::Left), None);
        assert_eq!(text_tone(&t, ShownKind::Equal, Side::Left), None);
        assert_eq!(text_tone(&t, ShownKind::Pending, Side::Right), None);
    }
}
