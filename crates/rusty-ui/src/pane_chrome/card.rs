use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    App, BoxShadow, Div, ElementId, InteractiveElement, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::tokens::{self, Theme};

const HEADER_HEIGHT: f32 = 42.;

#[must_use]
pub fn card_shadow(t: &Theme, is_active: bool) -> Vec<BoxShadow> {
    if is_active {
        t.shadows.card_active.clone()
    } else {
        t.shadows.card.clone()
    }
}

pub fn pane_card(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    is_active: bool,
    cx: &App,
) -> ObservedElement<Stateful<Div>> {
    let t = tokens::current(cx);
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(label.into())
        .aria_selected(is_active)
        .test_support()
        .flex()
        .flex_col()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .rounded(t.radii.card)
        .bg(t.surfaces.card)
        .shadow(card_shadow(t, is_active))
}

#[must_use]
pub fn card_header(t: &Theme) -> Div {
    div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(HEADER_HEIGHT))
        .px(t.spacing.md)
}
