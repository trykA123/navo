//! One card line: side by side (two halves) or unified (a sign column and one text).
//! Tints come from the diff tokens; a filler half uses the gap hatch colour. A line whose card
//! has a parsed snippet draws its syntax colour runs, as the file body does (D38.3).

use gpui_kit::{Div, Hsla, StyledText};

use crate::compare_diff::edge::edged;
use crate::tokens::DiffTone;

use super::syntax::TextRun;
use super::{accent_bar, frame};
use crate::compare_diff::hooks::{DiffRow, Layout};
use crate::compare_diff::rows::Half;
use crate::prelude::*;
use crate::tokens::Theme;

/// Height of a code line, in px.
pub(crate) const LINE_HEIGHT: f32 = 20.;
/// Width of a line-number gutter, in px.
const GUTTER: f32 = 44.;
/// Width of the sign column in unified, in px.
const SIGN: f32 = 18.;

/// The two sides' texts of a line and their colour runs; `None` text is a filler.
#[derive(Clone, Copy)]
pub(super) struct Texts<'a> {
    pub left: Option<&'a str>,
    pub right: Option<&'a str>,
    /// The syntax colour runs of each side's line, in bytes of its text; empty draws it plain.
    pub left_runs: &'a [TextRun],
    /// The same for the right side.
    pub right_runs: &'a [TextRun],
}

/// What one half shows: its line number, text, the tone of its text column, the colour runs of
/// that text and the hatching of a half with no line.
struct HalfView<'a> {
    number: Option<u32>,
    text: Option<&'a str>,
    runs: &'a [TextRun],
    tone: Option<DiffTone>,
    hatch: Option<Hsla>,
}

/// How a line is drawn: layout, which half, whether it is the current hunk, whether
/// it ends the card, and whether its text wraps instead of truncating (W8k).
#[derive(Clone, Copy)]
pub(super) struct Look {
    pub layout: Layout,
    pub half: Half,
    pub current: bool,
    pub last: bool,
    pub wrap: bool,
}

/// Builds a line row. With wrap the row keeps its height as a floor and grows with its text;
/// gpui's flex row then stretches both halves to the taller one.
pub(super) fn view(t: &Theme, look: Look, row: DiffRow, text: Texts<'_>) -> Div {
    let body = match look.layout {
        Layout::SideBySide => sides(t, row, text, look.wrap),
        Layout::Unified => unified(t, row, text, look),
    };
    let frame = frame(t, look.last);
    let frame = if look.wrap {
        frame.min_h(px(LINE_HEIGHT)).items_stretch()
    } else {
        frame.h(px(LINE_HEIGHT))
    };
    frame
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .when(look.current, |d| d.child(accent_bar(t)))
        .child(body)
}

/// The tone of each half's text column: a change on both, a deletion left, an insertion right.
pub(super) fn half_tones(t: &Theme, row: DiffRow) -> (Option<DiffTone>, Option<DiffTone>) {
    match row {
        DiffRow::Equal(..) => (None, None),
        DiffRow::Changed(..) => (Some(t.diff.change), Some(t.diff.change)),
        DiffRow::Deleted(_) => (Some(t.diff.delete), None),
        DiffRow::Inserted(_) => (None, Some(t.diff.add)),
    }
}

/// Both halves of a side-by-side line. `wrap` lets them grow past one row.
fn sides(t: &Theme, row: DiffRow, text: Texts<'_>, wrap: bool) -> Div {
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
        runs: text.left_runs,
        tone: lt,
        hatch: hatch.filter(|_| l.is_none()),
    };
    let right = HalfView {
        number: r,
        text: text.right,
        runs: text.right_runs,
        tone: rt,
        hatch: hatch.filter(|_| r.is_none()),
    };
    let body = div().flex().w_full();
    let body = if wrap {
        body.items_stretch()
    } else {
        body.h_full()
    };
    body.child(half_view(t, &left, true, wrap))
        .child(half_view(t, &right, false, wrap))
}

/// One half of a side-by-side line; the right half has a left edge. Wrapped, its text and
/// gutter start at the top and the cell itself grows with the text.
fn half_view(t: &Theme, half: &HalfView<'_>, first: bool, wrap: bool) -> Div {
    let cell = div().flex().flex_1().min_w_0().overflow_hidden();
    let cell = if wrap {
        cell.items_start()
    } else {
        cell.items_center().h_full()
    };
    cell.when(!first, |d| d.border_l_1().border_color(t.colors.line))
        .when_some(half.hatch, Styled::bg)
        .child(gutter(t, half.number))
        .child(edged(code(t, half.text, half.runs, wrap), half.tone))
}

/// What a unified line shows: its two numbers, its sign, the tone of its text column and the
/// text with its colour runs.
#[derive(Clone, Copy)]
struct ShownLine<'a> {
    left: Option<u32>,
    right: Option<u32>,
    sign: &'static str,
    tone: Option<DiffTone>,
    text: Option<&'a str>,
    runs: &'a [TextRun],
}

/// The one side of `row` that unified draws at `half`, with its numbers, sign, tone, text and
/// colour runs. Equal lines and the left half of a change draw the left text, the right half of
/// a change and an insertion draw the right text, a deletion draws the left one with its sign.
fn shown_line<'a>(t: &Theme, row: DiffRow, text: Texts<'a>, half: Half) -> ShownLine<'a> {
    let plain = ShownLine {
        left: None,
        right: None,
        sign: "",
        tone: None,
        text: text.left,
        runs: text.left_runs,
    };
    let deleted = ShownLine {
        sign: "\u{2212}",
        tone: Some(t.diff.delete),
        ..plain
    };
    let added = ShownLine {
        left: None,
        right: None,
        sign: "+",
        tone: Some(t.diff.add),
        text: text.right,
        runs: text.right_runs,
    };
    match (row, half) {
        (DiffRow::Equal(l, r), _) | (DiffRow::Changed(l, r), Half::Both) => ShownLine {
            left: Some(l),
            right: Some(r),
            ..plain
        },
        (DiffRow::Changed(l, _) | DiffRow::Deleted(l), _) if half != Half::Right => ShownLine {
            left: Some(l),
            ..deleted
        },
        (DiffRow::Changed(_, r) | DiffRow::Inserted(r), _) => ShownLine {
            right: Some(r),
            ..added
        },
        (DiffRow::Deleted(l), _) => ShownLine {
            left: Some(l),
            ..deleted
        },
    }
}

/// A unified line: two gutters, a sign and one text. Equal lines show both numbers.
/// Wrapped, the text grows past its row and the gutters and sign stay at the top.
fn unified(t: &Theme, row: DiffRow, text: Texts<'_>, look: Look) -> Div {
    let ShownLine {
        left,
        right,
        sign,
        tone,
        text,
        runs,
    } = shown_line(t, row, text, look.half);
    let body = div().flex().w_full();
    let body = if look.wrap {
        body.items_start()
    } else {
        body.items_center().h_full()
    };
    let cell = div().flex().flex_1().min_w_0();
    let cell = if look.wrap {
        cell.items_start()
    } else {
        cell.items_center().h_full()
    };
    body.child(gutter(t, left))
        .child(gutter(t, right))
        .child(edged(
            cell.child(
                div()
                    .flex_none()
                    .w(px(SIGN))
                    .text_color(tone.map_or(t.colors.muted, |tone| tone.text))
                    .child(sign),
            )
            .child(code(t, text, runs, look.wrap)),
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

/// The code text of one half; a filler shows nothing. Colour runs paint the words as the file
/// body's `line_text` does, and stay on the line when it wraps (W8k); only unwrapped lines
/// truncate.
fn code(t: &Theme, text: Option<&str>, runs: &[TextRun], wrap: bool) -> Div {
    let cell = div().flex_1().min_w_0().text_color(t.colors.text);
    let cell = if wrap { cell } else { cell.truncate() };
    let Some(text) = text else {
        return cell;
    };
    let styled = if runs.is_empty() {
        SharedString::from(text.to_owned()).into_any_element()
    } else {
        StyledText::new(text)
            .with_highlights(runs.iter().cloned())
            .into_any_element()
    };
    cell.child(styled)
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
