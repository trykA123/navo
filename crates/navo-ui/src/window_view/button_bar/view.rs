//! Button bar strip view, overflow `»` popup menu, and button tooltips.

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, App, BoxShadow, Context, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Role, SharedString, Stateful, StatefulInteractiveElement, Styled,
    TextRun, Window, anchored, deferred, div, font, point, px,
};
use navo_l10n::tr;

use super::drop::{DragVisual, NON_TAKER_OPACITY, drop_chip, item_takes_files, wire_button_drop};
use super::fit::{self, BarMetrics, Slot};
use super::icon::icon_element;
use super::{
    ButtonAction, ButtonItem, MSG_BAR_LABEL, MSG_EDIT_BUTTONS, MSG_MORE, MSG_TAKES_FILES,
    MSG_UNKNOWN_COMMAND,
};
use crate::commands;
use crate::primitives::fit::Fit;
use crate::primitives::popup_menu::{
    ChooseHandler, Keys, PopupMenu, choose, note_items, popup_state, toggle, wire_anchor, wire_keys,
};
use crate::primitives::{Button, ButtonVariant, Tooltip};
use crate::settings::Field;
use crate::tokens::{self, Theme};
use crate::window_view::{NavoWindow, UserCommandInfo};

const MORE_GLYPH: &str = "»";
const MORE_MENU_WIDTH: f32 = 240.;
const MORE_ROW_HEIGHT: f32 = 30.;
const DIMMED_OPACITY: f32 = 0.45;
const FOCUS_RING_PX: f32 = 2.;

/// Returns the GPUI action name for `id` when it is runnable from the button bar.
#[must_use]
pub(crate) fn runnable(id: &str) -> Option<&'static str> {
    if id == "user_commands.run" || id.starts_with("button_bar.") {
        return None;
    }
    let spec = commands::by_name(id)?;
    let action = spec.action?;
    matches!(spec.context, commands::GLOBAL | "FileList" | "NavoPane").then_some(action)
}

/// Formats tooltip text for `item` using live `user_commands` metadata.
#[must_use]
pub(crate) fn tooltip_text(item: &ButtonItem, user_commands: &[UserCommandInfo]) -> String {
    let mut lines = vec![item.name.clone()];
    match &item.action {
        ButtonAction::Navo(id) if runnable(id).is_none() => {
            lines.push(tr(MSG_UNKNOWN_COMMAND));
        }
        ButtonAction::User(name) => {
            if let Some(info) = user_commands.iter().find(|cmd| cmd.name == *name) {
                if let Some(key) = &info.key {
                    lines.push(commands::shown(key).unwrap_or_else(|| key.clone()));
                }
                if info.takes_files {
                    lines.push(tr(MSG_TAKES_FILES));
                }
            }
        }
        ButtonAction::Program {
            line, takes_files, ..
        } => {
            if !line.is_empty() {
                lines.push(line.clone());
            }
            if *takes_files {
                lines.push(tr(MSG_TAKES_FILES));
            }
        }
        ButtonAction::Navo(_) | ButtonAction::Menu(_) | ButtonAction::Separator => {}
    }
    lines.join("\n")
}

struct BarData {
    view: Entity<NavoWindow>,
    items: Vec<ButtonItem>,
    slots: Vec<Slot>,
    user_commands: Vec<UserCommandInfo>,
    focused: Option<usize>,
    drag: DragVisual,
    show_names: bool,
    metrics: BarMetrics,
    popup: Entity<PopupMenu>,
}

struct MoreMenu<'a> {
    view: Entity<NavoWindow>,
    theme: &'a Theme,
    metrics: BarMetrics,
    folded: &'a [(usize, &'a ButtonItem)],
    user_commands: &'a [UserCommandInfo],
    popup: Entity<PopupMenu>,
}

struct MoreRowProps<'a> {
    theme: &'a Theme,
    metrics: BarMetrics,
    position: usize,
    is_selected: bool,
    popup: Entity<PopupMenu>,
    on_choose: ChooseHandler,
}

impl NavoWindow {
    /// Builds the button bar strip when enabled and not covered by a full-window view.
    pub(in crate::window_view) fn button_bar_view(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.button_bar_shown(cx) {
            return None;
        }
        let popup = popup_state("button-bar-more", window, cx);
        let metrics = fit::metrics(&tokens::current(cx).spacing, tokens::current(cx).density);
        if self.button_bar.items.is_empty() {
            let empty =
                Button::new("button-bar-edit", tr(MSG_EDIT_BUTTONS)).variant(ButtonVariant::Ghost);
            return Some(self.bar_shell(metrics, cx).child(empty).into_any_element());
        }
        let show_names = self.values.flag(Field::WindowButtonBarNames);
        let slots = {
            let t = tokens::current(cx);
            measure_slots(&self.button_bar.items, show_names, metrics, window, t)
        };
        let focused = self
            .button_bar
            .focused
            .filter(|_| self.button_bar.focus.is_focused(window));
        let data = BarData {
            view: cx.entity(),
            items: self.button_bar.items.clone(),
            slots,
            user_commands: self.user_command_list().to_vec(),
            focused,
            drag: self.button_bar_drag_visual(),
            show_names,
            metrics,
            popup,
        };
        let row = Fit::new(metrics.height, move |width, _, cx| {
            build_row(data, width, cx)
        });
        Some(self.bar_shell(metrics, cx).child(row).into_any_element())
    }

    fn bar_shell(
        &self,
        metrics: BarMetrics,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<gpui_kit::Div>> {
        let bar = self.wire_bar_drag(self.button_bar_keys(div().id("button-bar"), cx), cx);
        let t = tokens::current(cx);
        bar.role(Role::Group)
            .aria_label(tr(MSG_BAR_LABEL))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(metrics.height)
            .px(t.spacing.xs)
            .bg(t.colors.rail)
    }
}

fn label_width(window: &Window, t: &Theme, text: &str) -> f32 {
    let mut face = font(t.typography.ui_font.clone());
    face.weight = FontWeight::NORMAL;
    let shared = SharedString::from(text.to_owned());
    let run = TextRun {
        len: shared.len(),
        font: face,
        color: t.colors.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .layout_line(&shared, t.typography.small, &[run], None);
    f32::from(line.width).ceil()
}

fn measure_slots(
    items: &[ButtonItem],
    show_names: bool,
    metrics: BarMetrics,
    window: &Window,
    t: &Theme,
) -> Vec<Slot> {
    let min_w = f32::from(metrics.button);
    let named_base =
        (f32::from(t.spacing.sm) * 2.0 + f32::from(t.spacing.xs) + f32::from(metrics.icon))
            .max(min_w);
    items
        .iter()
        .map(|item| match item.action {
            ButtonAction::Separator => Slot::Separator,
            _ if show_names => Slot::Button(named_base + label_width(window, t, &item.name)),
            _ => Slot::Button(min_w),
        })
        .collect()
}

fn build_row(data: BarData, width: crate::Pixels, cx: &mut App) -> AnyElement {
    let m = data.metrics;
    let shown = fit::fit(
        &data.slots,
        f32::from(width),
        f32::from(m.button),
        f32::from(m.separator),
        f32::from(m.gap),
    );
    let has_folded = data
        .items
        .iter()
        .skip(shown)
        .any(|item| !matches!(item.action, ButtonAction::Separator));
    note_items(&data.popup, has_folded, cx);
    let t = tokens::current(cx);
    let folded: Vec<(usize, &ButtonItem)> = data
        .items
        .iter()
        .enumerate()
        .skip(shown)
        .filter(|(_, item)| !matches!(item.action, ButtonAction::Separator))
        .collect();

    let visible = data.items[..shown].iter().enumerate().map(|(index, item)| {
        if matches!(item.action, ButtonAction::Separator) {
            return separator_element(index, t, m);
        }
        button_element(index, item, &data, t, cx)
    });

    let more = (!folded.is_empty()).then(|| {
        more_button(
            MoreMenu {
                view: data.view.clone(),
                theme: t,
                metrics: m,
                folded: &folded,
                user_commands: &data.user_commands,
                popup: data.popup.clone(),
            },
            cx,
        )
    });

    div()
        .flex()
        .items_center()
        .w(width)
        .h(m.height)
        .gap(m.gap)
        .children(visible)
        .children(more)
        .into_any_element()
}

fn separator_element(index: usize, t: &Theme, metrics: BarMetrics) -> AnyElement {
    div()
        .id(("button-bar-sep", index))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .w(metrics.separator)
        .h(metrics.button)
        .child(div().w(px(1.)).h(metrics.icon).bg(t.colors.line))
        .into_any_element()
}

fn button_element(
    index: usize,
    item: &ButtonItem,
    data: &BarData,
    t: &Theme,
    cx: &App,
) -> AnyElement {
    let m = data.metrics;
    let is_dimmed = is_non_runnable(item);
    let is_focused = data.focused == Some(index);
    let takes = item_takes_files(item, &data.user_commands);
    let drag_taker = data.drag.active && takes;
    let drag_dimmed = data.drag.active && !takes;
    let show_chip = drag_taker && data.drag.hovered == Some(index) && data.drag.count > 0;
    let tip: SharedString = tooltip_text(item, &data.user_commands).into();
    let (hover, pressed) = (t.colors.hover, t.colors.selection);
    let (view, clicked) = (data.view.clone(), item.clone());
    let button = div()
        .id(("button-bar-item", index))
        .role(Role::Button)
        .aria_label(item.name.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(m.button)
        .when(data.show_names, |d| d.px(t.spacing.sm).gap(t.spacing.xs))
        .when(!data.show_names, |d| d.w(m.button))
        .rounded(t.radii.sm)
        .when(is_focused, |d| d.shadow(focus_ring(t.colors.accent)))
        .when(!is_focused && drag_taker, |d| {
            d.shadow(focus_ring(t.surfaces.card_ring))
        })
        .when(drag_dimmed, |d| d.opacity(NON_TAKER_OPACITY))
        .when(!drag_dimmed && is_dimmed, |d| d.opacity(DIMMED_OPACITY))
        .when(is_dimmed, |d| d.text_color(t.colors.muted))
        .when(!is_dimmed, |d| {
            d.text_color(t.colors.text)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .active(move |s| s.bg(pressed))
        })
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, window, cx| {
            view.update(cx, |view, cx| view.run_button(&clicked, None, window, cx));
        })
        .child(icon_element(&item.icon, m.icon, cx))
        .when(data.show_names, |d| {
            d.child(div().text_size(t.typography.small).child(item.name.clone()))
        });
    let button = if takes {
        wire_button_drop(button, index, item, data.view.clone())
    } else {
        button
    };
    if !show_chip {
        return button.into_any_element();
    }
    div()
        .relative()
        .flex_none()
        .child(button)
        .child(drop_chip(&item.name, data.drag.count, m, t))
        .into_any_element()
}

fn more_button(menu: MoreMenu<'_>, cx: &App) -> AnyElement {
    let t = menu.theme;
    let m = menu.metrics;
    let is_open = menu.popup.read(cx).is_open;
    let toggle_state = menu.popup.clone();
    let hover = t.colors.hover;
    let button = div()
        .id("button-bar-more")
        .role(Role::Button)
        .aria_label(tr(MSG_MORE))
        .aria_expanded(is_open)
        .test_support();
    let button = wire_anchor(button, (&menu.popup, t.colors.accent), cx)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(m.button)
        .rounded(t.radii.sm)
        .text_color(t.colors.muted)
        .cursor_pointer()
        .when(is_open, |d| d.bg(hover))
        .hover(move |s| s.bg(hover))
        .on_click(move |event, window, cx| toggle(&toggle_state, event, window, cx))
        .child(MORE_GLYPH);
    let popup = is_open.then(|| more_popup(menu, cx));
    div()
        .relative()
        .flex_none()
        .child(button)
        .children(popup)
        .into_any_element()
}

fn more_popup(menu: MoreMenu<'_>, cx: &App) -> AnyElement {
    let t = menu.theme;
    let m = menu.metrics;
    let selected = menu.popup.read(cx).selected;
    let count = menu.folded.len();
    let folded_items: Vec<ButtonItem> = menu
        .folded
        .iter()
        .map(|(_, item)| (*item).clone())
        .collect();
    let view = menu.view;
    let on_choose: ChooseHandler = Rc::new(move |position, window, cx| {
        if let Some(item) = folded_items.get(position) {
            view.update(cx, |view, cx| view.run_button(item, None, window, cx));
        }
    });
    let rows: Vec<AnyElement> = menu
        .folded
        .iter()
        .enumerate()
        .map(|(position, &(index, item))| {
            let tip = tooltip_text(item, menu.user_commands);
            let props = MoreRowProps {
                theme: t,
                metrics: m,
                position,
                is_selected: position == selected,
                popup: menu.popup.clone(),
                on_choose: on_choose.clone(),
            };
            more_row(index, item, tip, props, cx)
        })
        .collect();
    let keys = Keys {
        state: menu.popup,
        count,
        on_choose,
    };
    let panel = wire_keys(div().id("button-bar-more-menu"), keys, cx)
        .role(Role::Menu)
        .aria_label(tr(MSG_MORE))
        .occlude()
        .flex()
        .flex_col()
        .w(px(MORE_MENU_WIDTH))
        .p(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.colors.surface)
        .shadow(t.shadows.menu.clone())
        .text_color(t.colors.text)
        .text_size(t.typography.small)
        .children(rows);
    let offset = point(m.button, m.height);
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

fn more_row(
    index: usize,
    item: &ButtonItem,
    tip: String,
    props: MoreRowProps<'_>,
    cx: &App,
) -> AnyElement {
    let theme = props.theme.selected_content(props.is_selected);
    let t = &theme;
    let hover = if props.is_selected && t.is_contrast {
        t.colors.selection
    } else {
        t.colors.hover
    };
    let is_dimmed = is_non_runnable(item);
    let (popup, on_choose, position) = (props.popup, props.on_choose, props.position);
    let tip: SharedString = tip.into();
    div()
        .id(("button-bar-more-row", index))
        .role(Role::MenuItem)
        .aria_label(item.name.clone())
        .aria_selected(props.is_selected)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(MORE_ROW_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .when(is_dimmed, |d| d.opacity(DIMMED_OPACITY))
        .when(!is_dimmed, |d| d.cursor_pointer())
        .when(props.is_selected, |d| {
            d.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .hover(move |s| s.bg(hover))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, window: &mut Window, cx| {
            choose(&popup, (position, &on_choose), window, cx);
        })
        .child(icon_element(&item.icon, props.metrics.icon, cx))
        .child(div().flex_1().min_w_0().truncate().child(item.name.clone()))
        .into_any_element()
}

fn is_non_runnable(item: &ButtonItem) -> bool {
    matches!(&item.action, ButtonAction::Navo(id) if runnable(id).is_none())
}

fn focus_ring(color: gpui_kit::Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(FOCUS_RING_PX),
        inset: true,
    }]
}
