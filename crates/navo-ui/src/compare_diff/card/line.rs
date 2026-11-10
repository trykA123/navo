//! One card line: side by side (two halves) or unified (a sign column and one text).
//! Tints come from the diff tokens; a filler half uses the gap hatch colour.

use gpui_kit::{Div, Hsla};

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

/// What one half shows: its line number, text and background.
struct HalfView<'a> {
    number: Option<u32>,
    text: Option<&'a str>,
    fill: Option<Hsla>,
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

/// Both halves of a side-by-side line.
fn sides(t: &Theme, row: DiffRow, text: Texts<'_>) -> Div {
    let (l, r, lf, rf) = match row {
        DiffRow::Equal(l, r) => (Some(l), Some(r), None, None),
        DiffRow::Changed(l, r) => (
            Some(l),
            Some(r),
            Some(t.diff.change.wash),
            Some(t.diff.change.wash),
        ),
        DiffRow::Deleted(l) => (
            Some(l),
            None,
            Some(t.diff.delete.wash),
            Some(t.diff.gap_hatch),
        ),
        DiffRow::Inserted(r) => (None, Some(r), Some(t.diff.gap_hatch), Some(t.diff.add.wash)),
    };
    let left = HalfView {
        number: l,
        text: text.left,
        fill: lf,
    };
    let right = HalfView {
        number: r,
        text: text.right,
        fill: rf,
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
        .when_some(half.fill, Styled::bg)
        .child(gutter(t, half.number))
        .child(code(t, half.text))
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
        .when_some(tone, |d, tone| d.bg(tone.wash))
        .child(gutter(t, l))
        .child(gutter(t, r))
        .child(
            div()
                .flex_none()
                .w(px(SIGN))
                .text_color(tone.map_or(t.colors.muted, |tone| tone.text))
                .child(sign),
        )
        .child(code(t, body))
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
