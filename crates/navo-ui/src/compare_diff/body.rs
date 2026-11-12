//! Draws the rows of the file diff: line numbers, text and the diff tints from tokens.
//! Side by side gives each side a number and a text column; unified gives two numbers and one text.

use gpui_kit::{Background, pattern_slash, rems};

use super::edge::edged;
use super::hooks::{Layout, PairText};
use super::long::{LongHit, OnBytesHere, bytes_here, is_long};
use super::rows::{ShownKind, ShownRow};
use super::spans::SpanCache;
use super::ticks::{PickSide, Ticks, tick_slot};
use crate::prelude::*;
use crate::tokens::Theme;
use cut::{LineAt, lead_row};
pub use cut::{ProbeSeen, RowCut, TextProbe};
pub(crate) use syntax::parse_side;
pub use syntax::{PairSyntax, SideSyntax, SyntaxLook};
use text::{line_text, span_runs, text_tone};

mod cut;
mod syntax;
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
    /// The sides' syntax parse and colours; `None` draws plain text (D38).
    pub syntax: Option<SyntaxLook<'a>>,
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

/// The hatching of a side with no line in a changed row or a filler; other sides have none.
fn side_hatch(t: &Theme, kind: ShownKind, side: Side) -> Option<Background> {
    match (kind, side) {
        (ShownKind::Deleted, Side::Right)
        | (ShownKind::Inserted, Side::Left)
        | (ShownKind::Filler, _) => Some(pattern_slash(t.diff.gap_hatch, 1., 4.)),
        _ => None,
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

/// The line `row` holds on `side`, if any.
fn pick<'a>(row: &ShownRow<'a>, side: Side) -> Option<(u32, &'a str)> {
    match side {
        Side::Left => row.left,
        Side::Right => row.right,
    }
}

/// One side of a side-by-side row, with its text cut as `row_cut` says.
fn side_cell(look: &BodyLook, row: &ShownRow, side: Side, row_cut: &RowCut) -> Div {
    let lead = lead_row(row, row_cut);
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .px(look.theme.spacing.sm)
        .when_some(side_hatch(look.theme, row.kind, side), |cell, fill| {
            cell.bg(fill)
        })
        .children(
            look.ticks
                .map(|ticks| tick_slot(look.theme, ticks, &lead, pick_side(side))),
        )
        .child(number(look, pick(&lead, side).map(|(line, _)| line)))
        .child(edged(
            line_text(
                look,
                &LineAt {
                    kind: row.kind,
                    side,
                    line: pick(row, side),
                    slot: side,
                },
                row_cut,
            ),
            text_tone(look.theme, row.kind, side),
        ))
        .children(long_button(look, &lead, (side, pick(&lead, side))))
}

/// The side a unified row draws: the inserted side when it has one, else the deleted.
fn unified_side<'a>(row: &ShownRow<'a>) -> (Side, Option<(u32, &'a str)>) {
    match row.right {
        Some(line) => (Side::Right, Some(line)),
        None => (Side::Left, row.left),
    }
}

/// The parts of a unified row after the border: tick, both numbers, the one text column and
/// its "Show bytes here" button. The text cell reports to the probe's `left` slot.
fn unified_cells(base: Div, look: &BodyLook, row: &ShownRow, row_cut: &RowCut) -> Div {
    let t = look.theme;
    let lead = lead_row(row, row_cut);
    let (side, line) = unified_side(row);
    base.px(t.spacing.sm)
        .children(look.ticks.map(|ticks| {
            let side = row.right.map_or(PickSide::Left, |_| PickSide::Right);
            tick_slot(t, ticks, &lead, side)
        }))
        .child(number(look, lead.left.map(|(line, _)| line)))
        .child(number(look, lead.right.map(|(line, _)| line)))
        .child(edged(
            line_text(
                look,
                &LineAt {
                    kind: row.kind,
                    side,
                    line,
                    slot: Side::Left,
                },
                row_cut,
            ),
            text_tone(t, row.kind, side),
        ))
        .children(long_button(look, &lead, unified_side(&lead)))
}

/// One drawn row of the file diff in the look's layout, with whole lines.
pub fn row_view(look: &BodyLook, row: &ShownRow) -> AnyElement {
    row_view_cut(look, row, &RowCut::default())
}

/// One drawn row of the file diff in the look's layout, with its text cut as `row_cut` says.
pub fn row_view_cut(look: &BodyLook, row: &ShownRow, row_cut: &RowCut) -> AnyElement {
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
            .child(side_cell(look, row, Side::Left, row_cut))
            .child(div().w(px(1.)).h_full().bg(t.colors.line))
            .child(side_cell(look, row, Side::Right, row_cut)),
        Layout::Unified => unified_cells(base, look, row, row_cut),
    };
    row_el.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{BodyLook, Side, side_hatch, span_runs};
    use crate::compare_diff::{
        DiffRow, Layout, PairFacts, PairText, RowSpans, ShownKind, SpanCache, ViewedKey,
    };
    use crate::tokens::{Mode, Theme};

    #[test]
    fn body_hatches_only_the_side_without_a_line() {
        let t = Theme::new(Mode::Light);
        assert!(side_hatch(&t, ShownKind::Equal, Side::Left).is_none());
        assert!(side_hatch(&t, ShownKind::Deleted, Side::Left).is_none());
        assert!(side_hatch(&t, ShownKind::Deleted, Side::Right).is_some());
        assert!(side_hatch(&t, ShownKind::Inserted, Side::Left).is_some());
        assert!(side_hatch(&t, ShownKind::Filler, Side::Right).is_some());
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
            syntax: None,
        };
        let line = Some((0, "héllo"));
        assert!(
            span_runs(&look(None), Side::Left, line).is_empty(),
            "Simple paints none"
        );
        assert_eq!(span_runs(&look(Some(&cache)), Side::Left, line), vec![3..4]);
    }
}
