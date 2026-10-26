use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, BoxShadow, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Role, SharedString, StatefulInteractiveElement, Styled, TextRun, Window, anchored, deferred,
    div, font, point, px,
};

use rusty_core::Location;
use rusty_l10n::tr;

use super::collapse::{Widths, hidden_segments};
use super::path_bar::NavigateHandler;
use super::segments::PathSegment;
use crate::primitives::popup_menu::{
    ChooseHandler, Keys, PopupMenu, choose, toggle, wire_anchor, wire_keys,
};
use crate::tokens::Theme;

pub(super) const CHEVRON: f32 = 12.;
const MENU_WIDTH: f32 = 260.;
const MENU_GAP: f32 = 4.;
const ELLIPSIS: &str = "…";

fn text_width(window: &Window, t: &Theme, text: &SharedString, weight: FontWeight) -> f32 {
    let mut face = font(t.typography.ui_font.clone());
    face.weight = weight;
    let run = TextRun {
        len: text.len(),
        font: face,
        color: t.colors.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .layout_line(text, t.typography.body, &[run], None);
    f32::from(line.width) + f32::from(t.spacing.xs) * 2.
}

fn menu_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui_kit::black().opacity(0.16),
        offset: point(px(0.), px(4.)),
        blur_radius: px(14.),
        spread_radius: px(0.),
        inset: false,
    }]
}

pub(super) struct Hidden {
    pub(super) items: Vec<(usize, PathSegment)>,
    pub(super) on_navigate: Option<NavigateHandler>,
    pub(super) state: Entity<PopupMenu>,
}

struct RowProps<'a> {
    theme: &'a Theme,
    position: usize,
    is_selected: bool,
    state: Entity<PopupMenu>,
    on_choose: ChooseHandler,
}

fn hidden_row(index: usize, item: PathSegment, props: RowProps) -> AnyElement {
    let t = props.theme;
    let c = &t.colors;
    let (state, on_choose, position) = (props.state, props.on_choose, props.position);
    div()
        .id(("path-hidden", index))
        .role(Role::MenuItem)
        .aria_label(item.label.clone())
        .aria_selected(props.is_selected)
        .test_support()
        .px(t.spacing.sm)
        .h(px(28.))
        .flex()
        .items_center()
        .rounded(t.radii.xs)
        .cursor_pointer()
        .when(props.is_selected, |d| d.bg(c.selection))
        .hover(|style| style.bg(c.hover))
        .truncate()
        .child(item.label)
        .on_click(move |_, window, cx| choose(&state, (position, &on_choose), window, cx))
        .into_any_element()
}

fn chooser(hidden: &Hidden) -> ChooseHandler {
    let targets: Vec<Location> = hidden
        .items
        .iter()
        .map(|(_, item)| item.location.clone())
        .collect();
    let on_navigate = hidden.on_navigate.clone();
    Rc::new(move |position, window, cx| {
        if let (Some(target), Some(handler)) = (targets.get(position), &on_navigate) {
            handler(target, window, cx);
        }
    })
}

pub(super) fn hidden_menu(hidden: Hidden, t: &Theme, cx: &App) -> AnyElement {
    let c = &t.colors;
    let on_choose = chooser(&hidden);
    let selected = hidden.state.read(cx).selected;
    let count = hidden.items.len();
    let rows: Vec<AnyElement> = hidden
        .items
        .into_iter()
        .enumerate()
        .map(|(position, (index, item))| {
            let props = RowProps {
                theme: t,
                position,
                is_selected: position == selected,
                state: hidden.state.clone(),
                on_choose: on_choose.clone(),
            };
            hidden_row(index, item, props)
        })
        .collect();
    let keys = Keys {
        state: hidden.state,
        count,
        on_choose,
    };
    let list = wire_keys(div().id("path-hidden-menu"), keys, cx)
        .role(Role::Menu)
        .aria_label(tr("path-hidden-menu"))
        .occlude()
        .flex()
        .flex_col()
        .w(px(MENU_WIDTH))
        .p(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(c.line)
        .bg(c.surface)
        .shadow(menu_shadow())
        .text_color(c.text)
        .text_size(t.typography.small)
        .children(rows);
    let menu = anchored()
        .offset(point(px(0.), px(MENU_GAP)))
        .snap_to_window_with_margin(px(8.))
        .child(list);
    div()
        .absolute()
        .top_full()
        .left_0()
        .child(deferred(menu).with_priority(1))
        .into_any_element()
}

pub(super) fn ellipsis_crumb(
    theme: &Theme,
    state: &Entity<PopupMenu>,
    menu: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let c = &theme.colors;
    let open = menu.is_some();
    let toggled = state.clone();
    let crumb = div()
        .id("path-more")
        .role(Role::Button)
        .aria_label(tr("path-hidden-show"))
        .aria_expanded(open)
        .test_support();
    let crumb = wire_anchor(crumb, (state, c.accent), cx)
        .flex_none()
        .px(theme.spacing.xs)
        .rounded(theme.radii.xs)
        .cursor_pointer()
        .text_color(c.muted)
        .bg(if open {
            c.hover
        } else {
            gpui_kit::transparent_black()
        })
        .hover(|style| style.bg(c.hover))
        .on_click(move |event, window, cx| toggle(&toggled, event, window, cx))
        .child(ELLIPSIS);
    div()
        .relative()
        .flex_none()
        .child(crumb)
        .children(menu)
        .into_any_element()
}

pub(super) struct Plan {
    pub(super) segments: Vec<PathSegment>,
    pub(super) hidden: Range<usize>,
}

pub(super) fn plan(segments: Vec<PathSegment>, window: &Window, t: &Theme, available: f32) -> Plan {
    let last = segments.len().saturating_sub(1);
    let crumbs: Vec<f32> = segments
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let weight = if index == last {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            };
            text_width(window, t, &item.label, weight)
        })
        .collect();
    let widths = Widths {
        crumbs: &crumbs,
        separator: CHEVRON + f32::from(t.spacing.xxs) * 2.,
        ellipsis: text_width(window, t, &ELLIPSIS.into(), FontWeight::NORMAL),
    };
    let hidden = if available > 0. {
        hidden_segments(&widths, available)
    } else {
        0..0
    };
    Plan { segments, hidden }
}
