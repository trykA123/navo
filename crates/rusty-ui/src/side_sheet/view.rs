use gpui_kit::base::TestSupportExt;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, Div, Empty, EntityId, InteractiveElement, IntoElement,
    ParentElement, Render, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::frame::Side;
use super::{CloseHandler, Hint};
use crate::primitives::{IconName, Kbd};
use crate::tokens::Theme;

const HEADER_HEIGHT: f32 = 56.;
const FOOTER_HEIGHT: f32 = 34.;
const CLOSE_HEIGHT: f32 = 28.;
const GRIP_WIDTH: f32 = 12.;
const GRIP_BAR: (f32, f32) = (4., 36.);
const GRIP_GROUP: &str = "side-sheet-grip";

#[derive(Clone, Copy)]
pub(super) struct DraggedGrip(pub EntityId);

struct GripGhost;

impl Render for GripGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub(super) fn grip(t: &Theme, side: Side, owner: EntityId) -> impl IntoElement {
    let inset = px(-GRIP_WIDTH / 2.);
    let accent = t.colors.accent;
    let grip = div()
        .id("side-sheet-grip")
        .role(Role::Splitter)
        .aria_label("Resize")
        .test_support()
        .group(GRIP_GROUP)
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(GRIP_WIDTH))
        .flex()
        .items_center()
        .justify_center()
        .cursor_col_resize()
        .on_drag(DraggedGrip(owner), |_, _, _, cx| cx.new(|_| GripGhost))
        .child(
            div()
                .w(px(GRIP_BAR.0))
                .h(px(GRIP_BAR.1))
                .rounded(t.radii.full)
                .bg(t.colors.faint)
                .group_hover(GRIP_GROUP, move |bar| bar.bg(accent)),
        );
    match side {
        Side::Right => grip.left(inset),
        Side::Left => grip.right(inset),
    }
}

fn close_button(
    t: &Theme,
    key: &SharedString,
    on_close: &CloseHandler,
) -> impl IntoElement + use<> {
    let on_close: CloseHandler = on_close.clone();
    let kbd = Kbd::new("side-sheet-close-key", key);
    let shortcut = kbd.as_ref().map(Kbd::text);
    let hover = (t.surfaces.field, t.colors.text);
    div()
        .id("side-sheet-close")
        .role(Role::Button)
        .aria_label("Close")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(CLOSE_HEIGHT))
        .pl(t.spacing.sm)
        .pr(t.spacing.xs)
        .rounded(t.radii.md)
        .text_color(t.colors.muted)
        .cursor_pointer()
        .hover(move |button| button.bg(hover.0).text_color(hover.1))
        .on_click(move |_, window, cx| on_close(window, cx))
        .child(Icon::new(IconName::Close).size(t.spacing.icon).flex_none())
        .children(kbd)
        .when_some(shortcut, |button, keys| button.aria_keyshortcuts(keys))
}

pub(super) fn header(
    t: &Theme,
    title: Vec<AnyElement>,
    close: Option<(SharedString, CloseHandler)>,
) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.lg)
        .min_w_0()
        .min_h(px(HEADER_HEIGHT))
        .py(t.spacing.lg)
        .pl(t.spacing.xl)
        .pr(t.spacing.md)
        .border_b_1()
        .border_color(t.surfaces.card_edge)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(t.spacing.lg)
                .children(title),
        )
        .children(close.map(|(key, on_close)| close_button(t, &key, &on_close)))
}

pub(super) fn body(content: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .children(content)
}

fn hint(t: &Theme, index: usize, hint: Hint) -> impl IntoElement + use<> {
    let keys: Vec<Kbd> = hint
        .keys
        .iter()
        .enumerate()
        .filter_map(|(at, key)| Kbd::new(("key", at), key))
        .collect();
    let spoken = if hint.label.is_empty() {
        keys.iter().map(Kbd::text).collect::<Vec<_>>().join(" ")
    } else {
        hint.label.to_string()
    };
    div()
        .id(("side-sheet-hint", index))
        .role(Role::Label)
        .aria_label(spoken)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .children(keys)
        .when(!hint.label.is_empty(), |chip| chip.child(hint.label))
}

fn status(t: &Theme, text: SharedString) -> impl IntoElement + use<> {
    div()
        .id("side-sheet-status")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex_none()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .text_color(t.colors.text)
        .child(text)
}

fn strip(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .rounded_b(t.radii.sheet)
        .bg(t.surfaces.field)
        .text_color(t.colors.muted)
        .text_size(t.typography.small)
        .whitespace_nowrap()
}

pub(super) fn footer(t: &Theme, hints: Vec<Hint>, text: Option<SharedString>) -> Option<Div> {
    if hints.is_empty() && text.is_none() {
        return None;
    }
    let chips = hints
        .into_iter()
        .enumerate()
        .map(|(index, entry)| hint(t, index, entry));
    let Some(text) = text else {
        return Some(
            strip(t)
                .items_center()
                .gap(t.spacing.xl)
                .h(px(FOOTER_HEIGHT))
                .px(t.spacing.xl)
                .overflow_hidden()
                .children(chips),
        );
    };
    Some(
        strip(t)
            .flex_col()
            .gap(t.spacing.sm)
            .px(t.spacing.xl)
            .py(t.spacing.md)
            .child(status(t, text))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(t.spacing.xl)
                    .gap_y(t.spacing.sm)
                    .children(chips),
            ),
    )
}
