use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    FontWeight, InteractiveElement, IntoElement, ParentElement, Role, StatefulInteractiveElement,
    Styled, div, px,
};

use super::{Argument, CHIP};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

pub(super) fn chip(t: &Theme, argument: &Argument) -> impl IntoElement {
    let theme = t.selected_content(true);
    let t = &theme;
    div()
        .id("command-center-chip")
        .role(Role::Label)
        .aria_label(argument.label.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .h(px(CHIP))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .bg(t.surfaces.current)
        .text_color(t.colors.accent_text)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .child(icon(
            IconName::SquareChevronRight,
            t.typography.small,
            t.colors.accent_text,
        ))
        .child(argument.label.clone())
}
