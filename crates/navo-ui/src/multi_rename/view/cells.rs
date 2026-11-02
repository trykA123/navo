use std::ops::Range;

use gpui_kit::{FontWeight, HighlightStyle, StyledText};

use super::preview::STATUS;
use super::status::Line;
use crate::prelude::*;
use crate::primitives::{ColumnSet, IconName, fit_name, icon};
use crate::tokens::Theme;

pub(super) fn name_cell(set: &ColumnSet, (at, text): (usize, &str), look: Look<'_>) -> Div {
    let fitted = fit_name("", text, set.text_chars(at));
    let is_whole = fitted.rest == text && !look.marks.is_empty();
    let body = div()
        .min_w_0()
        .truncate()
        .text_color(look.color)
        .when(look.is_struck, Styled::line_through);
    let body = if is_whole {
        let runs: Vec<_> = look
            .marks
            .iter()
            .map(|range| (range.clone(), look.found))
            .collect();
        body.child(StyledText::new(fitted.rest).with_highlights(runs))
    } else {
        body.child(fitted.rest)
    };
    set.cell(at).child(body)
}

#[derive(Clone, Copy)]
pub(super) struct Look<'a> {
    pub color: Hsla,
    pub is_struck: bool,
    pub marks: &'a [Range<usize>],
    pub found: HighlightStyle,
}

pub(super) fn found_style(t: &Theme) -> HighlightStyle {
    HighlightStyle {
        color: Some(t.colors.accent_text),
        font_weight: Some(FontWeight::MEDIUM),
        ..HighlightStyle::default()
    }
}

pub(super) fn status_cell(set: &ColumnSet, t: &Theme, line: &Line, color: Hsla) -> Div {
    let text = div()
        .min_w_0()
        .truncate()
        .text_color(if line.is_long { t.colors.text } else { color })
        .child(line.status());
    let row = div().flex().items_center().gap(t.spacing.xs).min_w_0();
    set.cell(STATUS).child(if line.is_long {
        row.child(icon(IconName::TriangleAlert, px(12.), t.colors.warn))
            .child(text)
    } else {
        row.child(text)
    })
}
