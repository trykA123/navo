use std::time::Duration;

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Animation, AnimationExt, AnyElement, BoxShadow, ClickEvent, Context, Div, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, point, px,
};
use rusty_core::shell_menu::Row;

use super::ContextMenu;
use super::motion::RootMotion;
use super::place::{CONTEXT_LINE, FIELD, FOOTER, Frame, PAD, ROW};
use super::row::{self, RowElement, RowLook};
use super::state::{ContextMenuEvent, LevelStatus};
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

const RADIUS: f32 = 10.;
const RISE: f32 = 4.;
const FILTER_HINT: &str = "↑↓ move · Enter run · → submenu · Esc clear";

fn shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui_kit::black().opacity(0.18),
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }]
}

fn chrome(t: &Theme, depth: usize) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("menu-level", depth))
        .role(Role::Menu)
        .test_support()
        .absolute()
        .flex()
        .flex_col()
        .p(px(PAD))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.surface)
        .shadow(shadow())
        .text_color(t.colors.text)
        .text_size(t.typography.body)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
}

struct Motion {
    id: SharedString,
    top: Pixels,
    duration: Duration,
    rise: f32,
}

fn enter(panel: ObservedElement<Stateful<Div>>, motion: Motion) -> AnyElement {
    let Motion {
        id,
        top,
        duration,
        rise,
    } = motion;
    let animation = Animation::new(duration).with_easing(ease_out_cubic);
    panel
        .with_animation(id, animation, move |panel, delta| {
            panel.top(top + px(rise * (1. - delta))).opacity(delta)
        })
        .into_any_element()
}

pub(super) struct PanelLook<'a> {
    pub frame: Frame,
    pub theme: &'a Theme,
    pub reduce_motion: bool,
    pub root: Option<RootMotion<'a>>,
}

fn interactive(
    element: RowElement,
    row: &Row,
    (depth, index): (usize, usize),
    cx: &mut Context<ContextMenu>,
) -> RowElement {
    if !row.is_selectable(true) {
        return element;
    }
    element
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.hovered(depth, index, cx);
            }
        }))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.clicked((depth, index), window, cx);
        }))
}

fn hint(state: &super::MenuState) -> String {
    if state.is_filtering() {
        return FILTER_HINT.to_owned();
    }
    match (state.hidden_count(), state.rare_count()) {
        (0, 0) => "Type to filter".to_owned(),
        (hidden, 0) => format!("{hidden} hidden: type to reach"),
        (0, rare) => format!("{rare} rare: type to reach"),
        (hidden, rare) => format!("{hidden} hidden, {rare} rare: type to reach"),
    }
}

impl ContextMenu {
    pub(super) fn panel(
        &self,
        depth: usize,
        look: PanelLook,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let PanelLook {
            frame,
            theme: t,
            reduce_motion,
            root,
        } = look;
        let fresh = root.as_ref().and_then(|root| root.fresh);
        let level = &self.state.levels()[depth];
        let list = div()
            .id("menu-list")
            .flex()
            .flex_col()
            .flex_none()
            .h(frame.list_height)
            .overflow_y_scroll()
            .track_scroll(&self.scroll(depth))
            .children(self.rows((depth, fresh), t, cx));
        let panel = chrome(t, depth)
            .left(frame.bounds.origin.x - self.bounds.origin.x)
            .w(frame.bounds.size.width)
            .h(frame.bounds.size.height)
            .when(depth == 0, |panel| {
                panel.child(self.filter(t)).child(self.context_line(t))
            })
            .child(list)
            .when(depth == 0, |panel| panel.child(self.footer(t, cx)));
        let top = frame.bounds.origin.y - self.bounds.origin.y;
        if let Some(root) = root {
            return panel
                .top(top + root.rise)
                .opacity(root.opacity)
                .into_any_element();
        }
        let motion = Motion {
            id: SharedString::from(format!("menu-enter-{:?}", level.path)),
            top,
            duration: Duration::from_millis(180),
            rise: if reduce_motion { 0. } else { RISE },
        };
        enter(panel, motion)
    }

    fn rows(
        &self,
        (depth, fresh): (usize, Option<(&[Row], f32)>),
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Vec<RowElement> {
        let level = &self.state.levels()[depth];
        let is_active = self.state.active() == depth;
        match level.status {
            LevelStatus::Loading => vec![row::status_row(t, 0, "Loading…", t.colors.muted)],
            LevelStatus::Failed => vec![
                row::status_row(t, 0, "Could not load", t.colors.muted),
                row::retry_row(t, 1, is_active).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.retried(depth, cx)),
                ),
            ],
            LevelStatus::Ready if level.rows.is_empty() => {
                vec![row::status_row(
                    t,
                    0,
                    "No matching commands",
                    t.colors.muted,
                )]
            }
            LevelStatus::Ready => {
                let look = RowLook {
                    theme: t,
                    menu: self.state.menu(),
                    icons: &self.icons,
                    is_active,
                };
                let selected = level.selection.index();
                level
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| {
                        let element = row::render(&look, row, index, selected == Some(index));
                        let element = match fresh {
                            Some((old, shown)) if !old.contains(row) => element.opacity(shown),
                            _ => element,
                        };
                        interactive(element, row, (depth, index), cx)
                    })
                    .collect()
            }
        }
    }

    fn filter(&self, t: &Theme) -> Div {
        let input = Input::new(&self.field)
            .id("menu-filter")
            .aria_label("Filter commands")
            .prefix(icon(IconName::Search, t.spacing.icon, t.colors.muted))
            .h(px(FIELD))
            .text_size(t.typography.body);
        let input = match Kbd::new("menu-escape-key", "escape") {
            Some(key) => input.suffix(key),
            None => input,
        };
        div().flex_none().h(px(FIELD)).child(input)
    }

    fn context_line(&self, t: &Theme) -> impl IntoElement {
        div()
            .id(("menu-context", 0usize))
            .role(Role::Label)
            .aria_label(self.context.clone())
            .test_support()
            .flex_none()
            .flex()
            .items_center()
            .h(px(CONTEXT_LINE))
            .px(t.spacing.sm)
            .text_size(t.typography.tiny)
            .text_color(t.colors.muted)
            .child(div().min_w_0().truncate().child(self.context.clone()))
    }

    fn footer(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let text = hint(&self.state);
        let classic = (self.has_classic && !self.state.is_filtering()).then(|| {
            div()
                .id("menu-classic")
                .role(Role::Button)
                .aria_label("Classic menu")
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.xs)
                .h(px(ROW - PAD * 2.))
                .px(t.spacing.xs)
                .rounded(t.radii.md)
                .cursor_pointer()
                .text_color(t.colors.text)
                .hover(|style| style.bg(t.colors.hover))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.conclude(Some(ContextMenuEvent::ShowClassic), window, cx);
                }))
                .child("Classic menu")
                .children(Kbd::new("menu-classic-key", "shift-f10"))
        });
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(t.spacing.sm)
            .h(px(FOOTER))
            .mt_auto()
            .pl(t.spacing.sm)
            .pr(t.spacing.xxs)
            .border_t_1()
            .border_color(t.colors.line)
            .text_size(t.typography.tiny)
            .text_color(t.colors.muted)
            .child(
                div()
                    .id(("menu-hint", 0usize))
                    .role(Role::Status)
                    .aria_label(text.clone())
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .child(text),
            )
            .children(classic)
    }
}
