use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::component::input::Input;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, div, point,
    px,
};
use rusty_core::shell_menu::Row;
use rusty_l10n::{t, tr};

use super::ContextMenu;
use super::place::{CONTEXT_LINE, FIELD, FOOTER, Frame, PAD, ROW};
use super::row::{self, RowElement, RowLook};
use super::state::{ContextMenuEvent, MenuState, PageStatus};
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

const RADIUS: f32 = 10.;

fn shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui_kit::black().opacity(0.18),
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }]
}

fn chrome(t: &Theme) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("menu-level", 0usize))
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

pub(super) struct PanelLook<'a> {
    pub frame: Frame,
    pub theme: &'a Theme,
    pub opacity: f32,
    pub shows_loading: bool,
}

fn interactive(
    element: RowElement,
    row: &Row,
    index: usize,
    cx: &mut Context<ContextMenu>,
) -> RowElement {
    if !row.is_selectable(true) {
        return element;
    }
    element
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.hovered(index, cx);
            }
        }))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.clicked(index, window, cx);
        }))
}

fn hint(state: &MenuState) -> String {
    if state.is_filtering() {
        return tr("context-menu-filter-keys");
    }
    if !state.page().is_empty() {
        return tr("context-menu-page-keys");
    }
    match state.menu().classic_only_count() {
        0 => tr("context-menu-type-to-filter"),
        count => t!("context-menu-classic-only", count = count),
    }
}

fn trail(state: &MenuState) -> Vec<String> {
    (1..=state.page().len())
        .filter_map(|depth| state.menu().item(&state.page()[..depth]))
        .map(|item| item.text.clone())
        .collect()
}

impl ContextMenu {
    pub(super) fn panel(&self, look: PanelLook, cx: &mut Context<Self>) -> AnyElement {
        let PanelLook {
            frame,
            theme: t,
            opacity,
            shows_loading,
        } = look;
        let list = div()
            .id("menu-list")
            .flex()
            .flex_col()
            .flex_none()
            .h(frame.list_height)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(self.rows(shows_loading, t, cx));
        let back = (!self.state.page().is_empty()).then(|| self.back_row(t, cx));
        let page = div()
            .flex()
            .flex_col()
            .flex_none()
            .relative()
            .child(self.context_line(t))
            .children(back)
            .child(list);
        chrome(t)
            .left(frame.bounds.origin.x - self.bounds.origin.x)
            .top(frame.bounds.origin.y - self.bounds.origin.y)
            .w(frame.bounds.size.width)
            .h(frame.bounds.size.height)
            .opacity(opacity)
            .child(self.filter(t))
            .child(page)
            .child(self.footer(t, cx))
            .into_any_element()
    }

    fn rows(&self, shows_loading: bool, t: &Theme, cx: &mut Context<Self>) -> Vec<RowElement> {
        let muted = t.colors.muted;
        match self.state.status() {
            PageStatus::Loading if shows_loading => {
                vec![row::status_row(t, 0, tr("context-menu-loading"), muted)]
            }
            PageStatus::Loading => Vec::new(),
            PageStatus::Failed => vec![
                row::status_row(t, 0, tr("context-menu-could-not-load"), muted),
                row::retry_row(t, 1, true).on_click(
                    cx.listener(|this, _: &ClickEvent, window, cx| this.retried(window, cx)),
                ),
            ],
            PageStatus::Ready => self.item_rows(t, cx),
        }
    }

    fn item_rows(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<RowElement> {
        let offset = usize::from(!self.state.page().is_empty());
        let listed = &self.state.rows()[offset.min(self.state.rows().len())..];
        if listed.is_empty() {
            let text = tr("context-menu-no-match");
            return vec![row::status_row(t, 0, text, t.colors.muted)];
        }
        let look = RowLook {
            theme: t,
            menu: self.state.menu(),
            icons: &self.icons,
        };
        let selected = self.state.selection().index();
        listed
            .iter()
            .enumerate()
            .map(|(shown, row)| {
                let index = shown + offset;
                let element = row::render(&look, row, shown, selected == Some(index));
                interactive(element, row, index, cx)
            })
            .collect()
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
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.tiny)
            .text_color(t.colors.muted)
            .child(div().min_w_0().truncate().child(self.context.clone()))
    }

    fn back_row(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let mut parts = trail(&self.state);
        let current = parts.pop().unwrap_or_default();
        let before = parts
            .iter()
            .map(|part| format!("{part} › "))
            .collect::<String>();
        div()
            .id("menu-back")
            .role(Role::Button)
            .aria_label(SharedString::from(current.clone()))
            .test_support()
            .flex_none()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .h(px(ROW))
            .px(t.spacing.sm)
            .rounded(t.radii.md)
            .cursor_pointer()
            .hover(|style| style.bg(t.colors.hover))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.went_back(window, cx)))
            .child(row::icon_cell(Some(
                icon(IconName::ChevronLeft, t.spacing.icon, t.colors.muted).into_any_element(),
            )))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .font_family(t.typography.mono_font.clone())
                    .text_size(t.typography.small)
                    .child(div().flex_none().text_color(t.colors.muted).child(before))
                    .child(div().min_w_0().text_ellipsis().child(current)),
            )
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
