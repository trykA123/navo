use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::component::input::Input;
use gpui_kit::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, InteractiveElement, IntoElement, MouseButton,
    MouseMoveEvent, ParentElement, Role, Stateful, StatefulInteractiveElement, Styled, div, point,
    px,
};
use rusty_core::shell_menu::Row;
use rusty_l10n::{t, tr};

use super::ContextMenu;
use super::flyout::{Bar, loading_bar};
use super::place::{CONTEXT_LINE, FIELD, FOOTER, Frame, PAD, ROW};
use super::row::{self, RowElement, RowLook};
use super::state::{ContextMenuEvent, MenuState, PageStatus, Spot};
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

pub(super) const RADIUS: f32 = 10.;

fn shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui_kit::black().opacity(0.18),
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }]
}

pub(super) fn chrome(t: &Theme, level: usize) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("menu-level", level))
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

/// How one panel looks this frame; `loading` is set once its loading cue is due.
pub(super) struct PanelLook<'a> {
    pub level: usize,
    pub frame: Frame,
    pub theme: &'a Theme,
    pub opacity: f32,
    pub loading: Option<Bar>,
}

fn interactive(
    element: RowElement,
    row: &Row,
    spot: Spot,
    cx: &mut Context<ContextMenu>,
) -> RowElement {
    if !row.is_selectable(true) {
        return element;
    }
    element
        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
            this.pointed(spot, event.position, cx);
        }))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.clicked(spot, window, cx);
        }))
}

fn hint(state: &MenuState) -> String {
    if state.is_filtering() {
        return tr("context-menu-filter-keys");
    }
    match state.menu().classic_only_count() {
        0 => tr("context-menu-type-to-filter"),
        count => t!("context-menu-classic-only", count = count),
    }
}

impl ContextMenu {
    pub(super) fn panel(&self, look: PanelLook, cx: &mut Context<Self>) -> AnyElement {
        let PanelLook {
            frame,
            theme: t,
            opacity,
            ..
        } = look;
        let page = div()
            .flex()
            .flex_col()
            .flex_none()
            .relative()
            .child(self.context_line(t))
            .child(self.list(&look, cx));
        chrome(t, 0)
            .left(frame.bounds.origin.x - self.bounds.origin.x)
            .top(frame.bounds.origin.y - self.bounds.origin.y)
            .w(frame.bounds.size.width)
            .h(frame.bounds.size.height)
            .opacity(opacity)
            .child(self.filter(t))
            .child(page)
            .child(self.footer(t, cx))
            .children(look.loading.map(|bar| loading_bar(t, 0, bar)))
            .into_any_element()
    }

    pub(super) fn list(&self, look: &PanelLook, cx: &mut Context<Self>) -> impl IntoElement {
        let scroll = &self.views[look.level].scroll;
        div()
            .id("menu-list")
            .flex()
            .flex_col()
            .flex_none()
            .h(look.frame.list_height)
            .overflow_y_scroll()
            .track_scroll(scroll)
            .children(self.rows(look, cx))
    }

    fn rows(&self, look: &PanelLook, cx: &mut Context<Self>) -> Vec<RowElement> {
        let (t, level) = (look.theme, look.level);
        let muted = t.colors.muted;
        match self.state.levels()[level].status() {
            PageStatus::Loading if look.loading.is_some() => {
                vec![row::status_row(t, 0, tr("context-menu-loading"), muted)]
            }
            PageStatus::Loading => Vec::new(),
            PageStatus::Failed => vec![
                row::status_row(t, 0, tr("context-menu-could-not-load"), muted),
                row::retry_row(t, 1, true).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| this.retried(level, window, cx),
                )),
            ],
            PageStatus::Ready => self.item_rows(t, level, cx),
        }
    }

    fn item_rows(&self, t: &Theme, level: usize, cx: &mut Context<Self>) -> Vec<RowElement> {
        let shown = &self.state.levels()[level];
        if shown.rows().is_empty() {
            let text = tr("context-menu-no-match");
            return vec![row::status_row(t, 0, text, t.colors.muted)];
        }
        let look = RowLook {
            theme: t,
            menu: self.state.menu(),
            icons: &self.icons,
        };
        let selected = shown.selection().index();
        shown
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let element = row::render(&look, row, index, selected == Some(index));
                interactive(element, row, Spot { level, row: index }, cx)
            })
            .collect()
    }

    fn filter(&self, t: &Theme) -> Div {
        let input = Input::new(&self.field)
            .id("menu-filter")
            .aria_label(tr("context-menu-filter-commands"))
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

    fn footer(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let text = hint(&self.state);
        let classic = (self.has_classic && !self.state.is_filtering()).then(|| {
            div()
                .id("menu-classic")
                .role(Role::Button)
                .aria_label(tr("context-menu-classic-menu"))
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
                .child(tr("context-menu-classic-menu"))
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
