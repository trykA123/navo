//! Draws the rows of the file diff: line numbers, text and the diff tints from tokens.
//! Side by side gives each side a number and a text column; unified gives two numbers and one text.

use std::ops::Range;

use gpui_kit::{Background, HighlightStyle, StyledText, pattern_slash, rems};

use super::hooks::{Layout, PairText};
use super::rows::{ShownKind, ShownRow};
use super::spans::SpanCache;
use super::ticks::{PickSide, Ticks, tick_slot};
use crate::prelude::*;
use crate::tokens::Theme;

/// Width of one digit of a line number, in rem (the mono font is about 0.6 em wide).
const DIGIT_REM: f32 = 0.6;

/// What every row of one frame shares.
pub struct BodyLook<'a> {
    pub theme: &'a Theme,
    pub layout: Layout,
    /// Digits of the widest line number.
    pub digits: usize,
    /// The current hunk; its rows carry a mark.
    pub current: Option<u32>,
    /// Changed spans inside lines; `None` in the Simple look, which paints none.
    pub spans: Option<&'a SpanCache>,
    /// The tick boxes of the picked lines; `None` where lines cannot be picked.
    pub ticks: Option<&'a Ticks<'a>>,
}

/// Digits of the largest line number on either side (at least 1).
#[must_use]
pub fn digits(text: &PairText) -> usize {
    text.left
        .len()
        .max(text.right.len())
        .max(1)
        .to_string()
        .len()
}

/// Which side of a row a cell draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

/// The pick side that goes with a drawn side.
fn pick_side(side: Side) -> PickSide {
    match side {
        Side::Left => PickSide::Left,
        Side::Right => PickSide::Right,
    }
}

/// The background of one side of a row: a wash for a change, hatching for a filler side.
fn side_fill(t: &Theme, kind: ShownKind, side: Side) -> Option<Background> {
    let wash = |tone: Hsla| Some(Background::from(tone));
    match (kind, side) {
        (ShownKind::Equal | ShownKind::Pending, _) => None,
        (ShownKind::Changed, _) => wash(t.diff.change.wash),
        (ShownKind::Deleted, Side::Left) => wash(t.diff.delete.wash),
        (ShownKind::Inserted, Side::Right) => wash(t.diff.add.wash),
        (ShownKind::Deleted | ShownKind::Inserted | ShownKind::Filler, _) => {
            Some(pattern_slash(t.diff.gap_hatch, 1., 4.))
        }
    }
}

/// The background of a unified row, which has one text column.
fn unified_fill(t: &Theme, kind: ShownKind) -> Option<Background> {
    match kind {
        ShownKind::Equal | ShownKind::Filler | ShownKind::Pending => None,
        ShownKind::Changed => Some(t.diff.change.wash.into()),
        ShownKind::Deleted => Some(t.diff.delete.wash.into()),
        ShownKind::Inserted => Some(t.diff.add.wash.into()),
    }
}

/// A right-aligned line number (1-based), or an empty cell of the same width.
fn number(look: &BodyLook, line: Option<u32>) -> Div {
    let t = look.theme;
    div()
        .flex_none()
        .w(rems(look.digits as f32 * DIGIT_REM))
        .mr(t.spacing.md)
        .text_right()
        .text_color(t.colors.faint)
        .children(line.map(|at| (u64::from(at) + 1).to_string()))
}

/// The text of one line, with `runs` (byte ranges) tinted; it never wraps.
fn line_text(text: Option<&str>, runs: Vec<Range<usize>>, tint: Hsla) -> Div {
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
fn span_runs(look: &BodyLook, side: Side, line: Option<(u32, &str)>) -> Vec<Range<usize>> {
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
fn span_tint(t: &Theme, kind: ShownKind) -> Hsla {
    match kind {
        ShownKind::Deleted => t.diff.delete.fill,
        ShownKind::Inserted => t.diff.add.fill,
        _ => t.diff.change.fill,
    }
}

/// One side of a side-by-side row.
fn side_cell(look: &BodyLook, row: &ShownRow, side: Side) -> Div {
    let at = match side {
        Side::Left => row.left,
        Side::Right => row.right,
    };
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .px(look.theme.spacing.sm)
        .when_some(side_fill(look.theme, row.kind, side), |cell, fill| {
            cell.bg(fill)
        })
        .children(
            look.ticks
                .map(|ticks| tick_slot(look.theme, ticks, row, pick_side(side))),
        )
        .child(number(look, at.map(|(line, _)| line)))
        .child(line_text(
            at.map(|(_, text)| text),
            span_runs(look, side, at),
            span_tint(look.theme, row.kind),
        ))
}

/// The one text column of a unified row: the inserted side when it has one, else the deleted.
fn unified_text(look: &BodyLook, row: &ShownRow) -> Div {
    let (side, line) = match row.right {
        Some(line) => (Side::Right, Some(line)),
        None => (Side::Left, row.left),
    };
    line_text(
        line.map(|(_, text)| text),
        span_runs(look, side, line),
        span_tint(look.theme, row.kind),
    )
}

/// One drawn row of the file diff in the look's layout.
pub fn row_view(look: &BodyLook, row: &ShownRow) -> AnyElement {
    let t = look.theme;
    let is_current = row.hunk.is_some() && row.hunk == look.current;
    let base = div()
        .flex()
        .size_full()
        .items_center()
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .when(row.kind == ShownKind::Pending, |row| {
            row.text_color(t.colors.muted)
        })
        .border_l_2()
        .border_color(if is_current {
            t.diff.change.mark
        } else {
            gpui_kit::transparent_black()
        });
    let row_el = match look.layout {
        Layout::SideBySide => base
            .child(side_cell(look, row, Side::Left))
            .child(div().w(px(1.)).h_full().bg(t.colors.line))
            .child(side_cell(look, row, Side::Right)),
        Layout::Unified => base
            .px(t.spacing.sm)
            .when_some(unified_fill(t, row.kind), |cell, fill| cell.bg(fill))
            .children(look.ticks.map(|ticks| {
                let side = row.right.map_or(PickSide::Left, |_| PickSide::Right);
                tick_slot(t, ticks, row, side)
            }))
            .child(number(look, row.left.map(|(line, _)| line)))
            .child(number(look, row.right.map(|(line, _)| line)))
            .child(unified_text(look, row)),
    };
    row_el.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{BodyLook, Side, side_fill, span_runs, unified_fill};
    use crate::compare_diff::{
        DiffRow, Layout, PairFacts, PairText, RowSpans, ShownKind, SpanCache, ViewedKey,
    };
    use crate::tokens::{Mode, Theme};

    #[test]
    fn body_tints_come_from_the_diff_tokens() {
        let t = Theme::new(Mode::Light);
        assert!(side_fill(&t, ShownKind::Equal, Side::Left).is_none());
        let wash = side_fill(&t, ShownKind::Deleted, Side::Left);
        assert_eq!(wash, Some(t.diff.delete.wash.into()));
        assert_ne!(side_fill(&t, ShownKind::Deleted, Side::Right), wash);
        assert_eq!(
            unified_fill(&t, ShownKind::Inserted),
            Some(t.diff.add.wash.into())
        );
    }

    #[test]
    fn body_paints_spans_only_when_given_and_skips_bad_ranges() {
        let t = Theme::new(Mode::Light);
        let text = PairText {
            left_starts: Vec::new(),
            left: vec!["héllo".into()],
            right: vec![],
            rows: vec![DiffRow::Changed(0, 0)],
            hunks: vec![],
            facts: PairFacts {
                left_encoding: "",
                right_encoding: "",
                left_endings: "",
                right_endings: "",
                raw_equal: false,
                display_equal: false,
                fallbacks: vec![],
            },
            viewed: ViewedKey([0; 64]),
        };
        let mut cache = SpanCache::default();
        cache.request(0..1);
        // 1..2 splits the two-byte "é"; 3..9 runs past the end; 3..4 is "l".
        let found = RowSpans {
            row: 0,
            left: vec![1..2, 3..9, 3..4],
            right: vec![],
        };
        cache.insert(&text, vec![found]);
        let look = |spans| BodyLook {
            theme: &t,
            layout: Layout::SideBySide,
            digits: 1,
            current: None,
            spans,
            ticks: None,
        };
        let line = Some((0, "héllo"));
        assert!(
            span_runs(&look(None), Side::Left, line).is_empty(),
            "Simple paints none"
        );
        assert_eq!(span_runs(&look(Some(&cache)), Side::Left, line), vec![3..4]);
    }
}
