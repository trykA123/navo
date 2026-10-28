use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, InteractiveElement, ParentElement, Role, StatefulInteractiveElement, Styled, div, px,
};

use super::CommandCenter;
use crate::primitives::Tooltip;
use crate::tokens::Theme;

impl CommandCenter {
    pub(super) fn content_chips(&self, t: &Theme) -> Div {
        div()
            .flex()
            .flex_wrap()
            .gap(t.spacing.xs)
            .w_full()
            .px(t.spacing.md)
            .children(
                self.props
                    .content
                    .chips
                    .iter()
                    .enumerate()
                    .map(|(index, chip)| render_chip(t, (index, chip))),
            )
    }
}

fn render_chip(
    t: &Theme,
    (index, chip): (usize, &super::super::content_source::ContentChip),
) -> impl gpui_kit::IntoElement {
    let color = if chip.message.is_some() {
        t.colors.mark
    } else {
        t.colors.accent_text
    };
    div()
        .id(("command-center-content-chip", index))
        .role(Role::Label)
        .aria_label(chip.label.clone())
        .test_support()
        .h(px(22.))
        .px(t.spacing.md)
        .rounded(t.radii.full)
        .bg(t.surfaces.field)
        .text_color(color)
        .text_size(t.typography.tiny)
        .when_some(chip.message.clone(), |element, message| {
            element.tooltip(move |window, cx| Tooltip::new(message.clone()).build(window, cx))
        })
        .child(chip.label.clone())
}
