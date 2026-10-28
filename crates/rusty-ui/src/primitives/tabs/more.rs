use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, App, Entity, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, anchored, deferred, div, point, px,
};

use rusty_l10n::t;

use super::plan::MORE_WIDTH;
use super::{SelectHandler, Tab};
use crate::primitives::icon;
use crate::primitives::popup_menu::{
    ChooseHandler, Keys, PopupMenu, choose, toggle, wire_anchor, wire_keys,
};
use crate::tokens::Theme;

const MENU_WIDTH: f32 = 240.;
const ROW_HEIGHT: f32 = 30.;

pub(super) struct Hidden {
    pub tabs: Vec<(usize, Tab)>,
    pub state: Entity<PopupMenu>,
    pub on_select: Option<SelectHandler>,
}

pub(super) fn more_tabs(t: &Theme, bar: &Theme, hidden: Hidden, cx: &App) -> AnyElement {
    let count = hidden.tabs.len();
    let toggle_state = hidden.state.clone();
    let is_open = hidden.state.read(cx).is_open;
    let hover = bar.colors.hover;
    let button = div()
        .id("tab-more")
        .role(Role::Button)
        .aria_label(t!("tabs-more", count = count))
        .aria_expanded(is_open)
        .test_support();
    let button = wire_anchor(button, (&hidden.state, t.colors.accent), cx)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(MORE_WIDTH))
        .h(t.spacing.tab_height)
        .rounded(t.radii.pill)
        .text_color(bar.colors.muted)
        .cursor_pointer()
        .when(is_open, |d| d.bg(hover))
        .hover(move |style| style.bg(hover))
        .on_click(move |event, window, cx| toggle(&toggle_state, event, window, cx))
        .child(format!("+{count}"));
    let menu = is_open.then(|| menu(t, hidden, cx));
    div()
        .relative()
        .flex_none()
        .child(button)
        .children(menu)
        .into_any_element()
}

fn chooser(hidden: &Hidden) -> ChooseHandler {
    let indexes: Vec<usize> = hidden.tabs.iter().map(|(index, _)| *index).collect();
    let on_select = hidden.on_select.clone();
    Rc::new(move |position, window, cx| {
        if let (Some(index), Some(handler)) = (indexes.get(position), &on_select) {
            handler(index, window, cx);
        }
    })
}

fn menu(t: &Theme, hidden: Hidden, cx: &App) -> AnyElement {
    let c = &t.colors;
    let on_choose = chooser(&hidden);
    let selected = hidden.state.read(cx).selected;
    let count = hidden.tabs.len();
    let rows: Vec<AnyElement> = hidden
        .tabs
        .into_iter()
        .enumerate()
        .map(|(position, tab)| {
            let props = RowProps {
                theme: t,
                position,
                is_selected: position == selected,
                state: hidden.state.clone(),
                on_choose: on_choose.clone(),
            };
            row(tab, props)
        })
        .collect();
    let keys = Keys {
        state: hidden.state,
        count,
        on_choose,
    };
    let panel = wire_keys(div().id("tab-more-menu"), keys, cx)
        .role(Role::Menu)
        .aria_label(t!("tabs-hidden-menu"))
        .occlude()
        .flex()
        .flex_col()
        .w(px(MENU_WIDTH))
        .p(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(c.line)
        .bg(c.surface)
        .shadow(t.shadows.menu.clone())
        .text_color(c.text)
        .text_size(t.typography.small)
        .children(rows);
    let offset = point(px(MORE_WIDTH), t.spacing.tab_height + px(4.));
    deferred(
        anchored()
            .anchor(Anchor::TopRight)
            .offset(offset)
            .snap_to_window_with_margin(px(8.))
            .child(panel),
    )
    .with_priority(1)
    .into_any_element()
}

struct RowProps<'a> {
    theme: &'a Theme,
    position: usize,
    is_selected: bool,
    state: Entity<PopupMenu>,
    on_choose: ChooseHandler,
}

fn row((index, tab): (usize, Tab), props: RowProps) -> AnyElement {
    let theme = props.theme.selected_content(props.is_selected);
    let t = &theme;
    let hover = if props.is_selected && t.is_contrast {
        t.colors.selection
    } else {
        t.colors.hover
    };
    let muted = t.colors.muted;
    let (state, on_choose, position) = (props.state, props.on_choose, props.position);
    let label: SharedString = tab.label.clone();
    div()
        .id(("tab-more-row", index))
        .role(Role::MenuItem)
        .aria_label(label.clone())
        .aria_selected(props.is_selected)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ROW_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(props.is_selected, |d| {
            d.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .hover(move |style| style.bg(hover))
        .on_click(move |_, window: &mut Window, cx| {
            choose(&state, (position, &on_choose), window, cx);
        })
        .when_some(tab.icon, |d, name| d.child(icon(name, px(14.), muted)))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .into_any_element()
}
