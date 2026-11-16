//! Shared look helpers for list rows: the inset focus ring and the cursor bar.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{BoxShadow, Hsla, InteractiveElement, IntoElement, Styled, div, point, px};

use crate::tokens::Theme;

pub(crate) fn inset_ring(color: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
        inset: true,
    }]
}

pub(crate) fn cursor_bar(t: &Theme) -> impl IntoElement + use<> {
    div()
        .id("cursor-bar")
        .test_support()
        .absolute()
        .left_0()
        .top(t.spacing.xs)
        .bottom(t.spacing.xs)
        .w(px(3.))
        .rounded(t.radii.xs)
        .bg(t.surfaces.card_ring)
}
