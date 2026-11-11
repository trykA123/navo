//! One cell of the folder compare's centre gutter (D33): a word pill on the pane's rail colour.
//!
//! Sizes follow `design/prototypes/compare-marks-d114.html`, direction 2 with words.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    StatefulInteractiveElement, Styled, div, px,
};

use super::gutter::GutterLabel;
use super::tone;
use crate::primitives::Tooltip;
use crate::tokens::Theme;

/// The gutter's width on a normal window.
const WIDE: Pixels = px(84.);
/// The gutter's width on a window under `NARROW_WINDOW`.
const NARROW: Pixels = px(66.);
const NARROW_WINDOW: Pixels = px(720.);
const PILL_HEIGHT: Pixels = px(20.);

/// The gutter width for a window of this width: 84 px, or 66 px under 720 px.
#[must_use]
pub fn gutter_width(window_width: Pixels) -> Pixels {
    if window_width < NARROW_WINDOW {
        NARROW
    } else {
        WIDE
    }
}

/// The pill's font size and side padding in px: `compact` is the narrow gutter,
/// where the smaller type keeps a label like "3 inside" inside the 66 px column.
fn pill_metrics(compact: bool) -> (Pixels, Pixels) {
    if compact {
        (px(10.5), px(4.))
    } else {
        (px(11.5), px(8.))
    }
}

/// The pill's border: the tone's text colour, or the quiet line colour for muted tones.
fn pill(t: &Theme, label: &GutterLabel, compact: bool) -> Div {
    let ink = tone::text(t, label.tone);
    let border = tone::wash(t, label.tone).map_or(t.colors.line, |_| ink);
    let (font, pad) = pill_metrics(compact);
    div()
        .h(PILL_HEIGHT)
        .px(pad)
        .flex()
        .items_center()
        .rounded(t.radii.full)
        .border_1()
        .border_color(border)
        .text_color(ink)
        .when_some(tone::wash(t, label.tone), Styled::bg)
        .text_size(font)
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .child(label.text.words())
}

/// The gutter cell of one row, `width` wide and as tall as its row; `None` leaves it empty.
///
/// `id` must be unique among the visible cells, as the hover text needs it.
#[must_use]
pub fn gutter_cell(
    t: &Theme,
    id: impl Into<ElementId>,
    width: Pixels,
    label: Option<&GutterLabel>,
) -> impl IntoElement {
    let compact = width < WIDE;
    let cell = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .w(width)
        .h_full()
        .bg(t.colors.rail)
        .border_l_1()
        .border_r_1()
        .border_color(t.colors.line);
    cell.when_some(label, |cell, label| {
        let tip = label.hover.clone();
        cell.child(
            pill(t, label, compact)
                .id(id.into())
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)),
        )
    })
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::{gutter_width, pill_metrics};

    #[test]
    fn the_gutter_is_84_px_wide_and_66_px_under_720() {
        assert_eq!(gutter_width(px(1440.)), px(84.));
        assert_eq!(gutter_width(px(720.)), px(84.));
        assert_eq!(gutter_width(px(719.)), px(66.));
        assert_eq!(gutter_width(px(390.)), px(66.));
    }

    #[test]
    fn the_pill_is_smaller_in_the_compact_gutter() {
        assert_eq!(pill_metrics(false), (px(11.5), px(8.)));
        assert_eq!(pill_metrics(true), (px(10.5), px(4.)));
    }
}
