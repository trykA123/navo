//! Draws the rows of the file diff: line numbers, text and the diff tints from tokens.
//! Side by side gives each side a number and a text column; unified gives two numbers and one text.

use gpui_kit::{Background, pattern_slash, rems};

use super::hooks::{Layout, PairText};
use super::long::{LongHit, OnBytesHere, bytes_here, is_long};
use super::rows::{ShownKind, ShownRow};
use super::spans::SpanCache;
use super::ticks::{PickSide, Ticks, tick_slot};
use crate::prelude::*;
use crate::tokens::Theme;
use text::{line_text, span_runs, span_tint};

mod text;

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
    /// The "Show bytes here" click for lines over 1 MiB; `None` where the pair cannot show bytes.
    pub bytes_here: Option<&'a OnBytesHere>,
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

/// The "Show bytes here" button for the line `at` of `row` when it is over 1 MiB.
fn long_button(
    look: &BodyLook,
    row: &ShownRow,
    (side, at): (Side, Option<(u32, &str)>),
) -> Option<AnyElement> {
    let on = look.bytes_here?;
    at.filter(|(_, text)| is_long(text))?;
    let hit = LongHit {
        left: row.left.map(|(line, _)| line),
        right: row.right.map(|(line, _)| line),
    };
    let name = match side {
        Side::Left => "compare-bytes-left",
        Side::Right => "compare-bytes-right",
    };
    Some(bytes_here(look.theme, (row.row, name), hit, on).into_any_element())
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
        .children(long_button(look, row, (side, at)))
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

/// The "Show bytes here" button of a unified row's one text line.
fn unified_button(look: &BodyLook, row: &ShownRow) -> Option<AnyElement> {
    match row.right {
        Some(line) => long_button(look, row, (Side::Right, Some(line))),
        None => long_button(look, row, (Side::Left, row.left)),
    }
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
            .child(unified_text(look, row))
            .children(unified_button(look, row)),
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
            right_starts: Vec::new(),
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
            bytes_here: None,
        };
        let line = Some((0, "héllo"));
        assert!(
            span_runs(&look(None), Side::Left, line).is_empty(),
            "Simple paints none"
        );
        assert_eq!(span_runs(&look(Some(&cache)), Side::Left, line), vec![3..4]);
    }
}
