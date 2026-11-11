//! One card line: side by side (two halves) or unified (a sign column and one text).
//! Tints come from the diff tokens; a filler half uses the gap hatch colour.

use gpui_kit::{Div, Hsla};

use crate::compare_diff::edge::edged;
use crate::tokens::DiffTone;

use super::{accent_bar, frame};
use crate::compare_diff::hooks::{DiffRow, Layout};
use crate::compare_diff::rows::Half;
use crate::prelude::*;
use crate::tokens::Theme;

/// Height of a code line, in px.
const LINE_HEIGHT: f32 = 20.;
/// Width of a line-number gutter, in px.
const GUTTER: f32 = 44.;
/// Width of the sign column in unified, in px.
const SIGN: f32 = 18.;

/// The two sides' texts of a line; `None` is a filler.
#[derive(Clone, Copy)]
pub(super) struct Texts<'a> {
    pub left: Option<&'a str>,
    pub right: Option<&'a str>,
}

/// What one half shows: its line number, text, the tone of its text column and the hatching
/// of a half with no line.
struct HalfView<'a> {
    number: Option<u32>,
    text: Option<&'a str>,
    tone: Option<DiffTone>,
    hatch: Option<Hsla>,
}

/// How a line is drawn: layout, which half, whether it is the current hunk, and whether
/// it ends the card.
#[derive(Clone, Copy)]
pub(super) struct Look {
    pub layout: Layout,
    pub half: Half,
    pub current: bool,
    pub last: bool,
}

/// Builds a line row.
pub(super) fn view(t: &Theme, look: Look, row: DiffRow, text: Texts<'_>) -> Div {
    let body = match look.layout {
        Layout::SideBySide => sides(t, row, text),
        Layout::Unified => unified(t, row, text, look.half),
    };
    frame(t, look.last)
        .h(px(LINE_HEIGHT))
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .when(look.current, |d| d.child(accent_bar(t)))
        .child(body)
}

/// The tone of each half's text column: a change on both, a deletion left, an insertion right.
fn half_tones(t: &Theme, row: DiffRow) -> (Option<DiffTone>, Option<DiffTone>) {
    match row {
        DiffRow::Equal(..) => (None, None),
        DiffRow::Changed(..) => (Some(t.diff.change), Some(t.diff.change)),
        DiffRow::Deleted(_) => (Some(t.diff.delete), None),
        DiffRow::Inserted(_) => (None, Some(t.diff.add)),
    }
}

/// Both halves of a side-by-side line.
fn sides(t: &Theme, row: DiffRow, text: Texts<'_>) -> Div {
    let (l, r) = match row {
        DiffRow::Equal(l, r) | DiffRow::Changed(l, r) => (Some(l), Some(r)),
        DiffRow::Deleted(l) => (Some(l), None),
        DiffRow::Inserted(r) => (None, Some(r)),
    };
    let (lt, rt) = half_tones(t, row);
    let hatch = Some(t.diff.gap_hatch);
    let left = HalfView {
        number: l,
        text: text.left,
        tone: lt,
        hatch: hatch.filter(|_| l.is_none()),
    };
    let right = HalfView {
        number: r,
        text: text.right,
        tone: rt,
        hatch: hatch.filter(|_| r.is_none()),
    };
    div()
        .flex()
        .w_full()
        .h_full()
        .child(half_view(t, &left, true))
        .child(half_view(t, &right, false))
}

/// One half of a side-by-side line; the right half has a left edge.
fn half_view(t: &Theme, half: &HalfView<'_>, first: bool) -> Div {
    div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .when(!first, |d| d.border_l_1().border_color(t.colors.line))
        .when_some(half.hatch, Styled::bg)
        .child(gutter(t, half.number))
        .child(edged(code(t, half.text), half.tone))
}

/// A unified line: two gutters, a sign and one text. Equal lines show both numbers.
fn unified(t: &Theme, row: DiffRow, text: Texts<'_>, half: Half) -> Div {
    let (l, r, sign, tone, body) = match (row, half) {
        (DiffRow::Equal(l, r), _) | (DiffRow::Changed(l, r), Half::Both) => {
            (Some(l), Some(r), "", None, text.left)
        }
        (DiffRow::Changed(l, _) | DiffRow::Deleted(l), _) if half != Half::Right => {
            (Some(l), None, "\u{2212}", Some(t.diff.delete), text.left)
        }
        (DiffRow::Changed(_, r) | DiffRow::Inserted(r), _) => {
            (None, Some(r), "+", Some(t.diff.add), text.right)
        }
        (DiffRow::Deleted(l), _) => (Some(l), None, "\u{2212}", Some(t.diff.delete), text.left),
    };
    div()
        .flex()
        .items_center()
        .w_full()
        .h_full()
        .child(gutter(t, l))
        .child(gutter(t, r))
        .child(edged(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .child(
                    div()
                        .flex_none()
                        .w(px(SIGN))
                        .text_color(tone.map_or(t.colors.muted, |tone| tone.text))
                        .child(sign),
                )
                .child(code(t, body)),
            tone,
        ))
}

/// A right-aligned line number (1-based) in the faint colour.
fn gutter(t: &Theme, number: Option<u32>) -> Div {
    div()
        .flex_none()
        .flex()
        .justify_end()
        .w(px(GUTTER))
        .pr(t.spacing.md)
        .text_color(t.colors.faint)
        .children(number.map(|n| SharedString::from((n + 1).to_string())))
}

/// The code text of one half; a filler shows nothing.
fn code(t: &Theme, text: Option<&str>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(t.colors.text)
        .children(text.map(|s| SharedString::from(s.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::half_tones;
    use crate::compare_diff::hooks::DiffRow;
    use crate::tokens::{Mode, Theme};

    #[test]
    fn half_tones_tint_changed_added_and_removed_text_only() {
        let t = Theme::new(Mode::Dark);
        assert_eq!(half_tones(&t, DiffRow::Equal(0, 0)), (None, None));
        let change = Some(t.diff.change);
        assert_eq!(half_tones(&t, DiffRow::Changed(0, 0)), (change, change));
        let delete = (Some(t.diff.delete), None);
        assert_eq!(half_tones(&t, DiffRow::Deleted(0)), delete);
        assert_eq!(
            half_tones(&t, DiffRow::Inserted(0)),
            (None, Some(t.diff.add))
        );
    }
}
