use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    Div, Entity, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, deferred, div, px, transparent_black,
};

use super::{MenuSlot, SlotMenu};
use crate::primitives::{IconName, TextField, TextFieldState, icon};
use crate::tokens::Theme;

pub(super) struct Cell<'a> {
    pub t: &'a Theme,
    pub label: String,
    pub input: &'a Entity<TextFieldState>,
    pub error: Option<&'a SharedString>,
}

pub(super) fn cell(id: &'static str, cell: Cell<'_>) -> Div {
    let Cell {
        t,
        label,
        input,
        error,
    } = cell;
    let border = if error.is_some() {
        t.colors.mark
    } else {
        transparent_black()
    };
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .child(caption(t, label.clone()))
        .child(
            div()
                .rounded(t.radii.sm)
                .border_1()
                .border_color(border)
                .child(TextField::new(id, input, label)),
        )
}

pub(super) fn attach(base: Div, slot: MenuSlot, menu: &mut SlotMenu) -> Div {
    if menu.as_ref().is_none_or(|(wanted, _)| *wanted != slot) {
        return base;
    }
    let Some((_, menu)) = menu.take() else {
        return base;
    };
    base.relative().child(
        deferred(div().absolute().top_full().left_0().pt(px(4.)).child(menu)).with_priority(1),
    )
}

pub(super) fn caption(t: &Theme, text: String) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

pub(super) fn error_line(
    t: &Theme,
    id: &'static str,
    error: &SharedString,
) -> impl IntoElement + use<> {
    div()
        .id(id)
        .role(Role::Alert)
        .aria_label(error.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.mark))
        .child(error.clone())
}
