use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, FocusHandle, InteractiveElement, IntoElement, ParentElement, Styled, Window, div, px,
    transparent_black,
};

use crate::primitives::Button;
use crate::tokens::Theme;

gpui_kit::actions!(
    dialog_focus,
    [Next, Previous, NextButton, PreviousButton, Press]
);

pub(crate) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "dialog_focus");
}

pub(crate) fn cycle(stops: &[FocusHandle], forward: bool, window: &mut Window, cx: &mut App) {
    let count = stops.len();
    let current = stops.iter().position(|stop| stop.is_focused(window));
    let next = match (current, forward) {
        (Some(index), true) => (index + 1) % count,
        (Some(index), false) => (index + count - 1) % count,
        (None, _) => 0,
    };
    if let Some(stop) = stops.get(next) {
        window.focus(stop, cx);
    }
}

pub(crate) struct Stop<'a> {
    pub id: &'static str,
    pub context: &'static str,
    pub handle: &'a FocusHandle,
    pub button: Button,
}

pub(crate) fn focus_ring(t: &Theme, stop: Stop<'_>, window: &Window) -> impl IntoElement {
    let ring = if stop.handle.is_focused(window) {
        t.colors.accent
    } else {
        transparent_black()
    };
    div()
        .id(stop.id)
        .key_context(stop.context)
        .test_support()
        .track_focus(stop.handle)
        .p(px(2.))
        .rounded(t.radii.lg)
        .border_2()
        .border_color(ring)
        .child(stop.button)
}
