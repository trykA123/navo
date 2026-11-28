//! Terminal header bar built on [`crate::primitives::Tabs`] and [`crate::primitives::popup_menu`].
//!
//! Shows session tabs with 14 px status indicators, New (`+`), a shell picker menu, and drawer
//! tools. Below [`HEADER_WRAP_WIDTH`] (760 px) the header wraps cleanly into two rows.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Anchor, Entity, Role, anchored, deferred, point};
use navo_term::ShellSpec;

use crate::prelude::*;
use crate::primitives::popup_menu::{
    ChooseHandler, Keys, PopupMenu, choose, toggle, wire_anchor, wire_keys,
};
use crate::primitives::{IconButton, IconName, Tab, TabStatus, Tabs, Tooltip, icon};
use crate::tokens::{self, Theme};

/// Terminal width threshold below which the header wraps into two rows (SPEC / NV13e).
pub const HEADER_WRAP_WIDTH: Pixels = px(760.0);

const NEW_BTN_WIDTH: f32 = 28.0;
const PICKER_BTN_WIDTH: f32 = 22.0;
const NEW_GROUP_WIDTH: f32 = NEW_BTN_WIDTH + PICKER_BTN_WIDTH + 4.0;
const PICKER_MENU_WIDTH: f32 = 232.0;
const PICKER_ROW_HEIGHT: f32 = 30.0;
const ICON_SIZE: f32 = 14.0;

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;
type IndexHandler = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
type ShellHandler = Rc<dyn Fn(ShellSpec, &mut Window, &mut App)>;

/// Visual model for one session tab in [`render_header`].
pub struct HeaderTab {
    pub label: SharedString,
    pub status: Option<TabStatus>,
    pub italic: bool,
    pub icon: IconName,
}

/// Data needed to render the terminal header for one frame.
pub(crate) struct HeaderModel {
    pub tabs: Vec<HeaderTab>,
    pub active: usize,
    pub is_focused: bool,
    pub is_two_row: bool,
    pub active_summary: SharedString,
    pub shells: Vec<ShellSpec>,
    pub default_shell: Option<usize>,
    pub shell_picker: Entity<PopupMenu>,
    pub tabs_menu: Entity<PopupMenu>,
}

/// Callbacks wired to the header controls.
pub(crate) struct HeaderActions {
    pub on_select: IndexHandler,
    pub on_close: IndexHandler,
    pub on_new: Callback,
    pub on_pick_shell: ShellHandler,
    pub on_hide: Callback,
}

/// Renders the terminal header in either single-row (`>= 760 px`) or two-row (`< 760 px`) mode.
pub(crate) fn render_header(model: HeaderModel, actions: HeaderActions, cx: &App) -> AnyElement {
    let t = tokens::current(cx);
    let is_two_row = model.is_two_row;
    let summary = model.active_summary.clone();
    let tabs_strip = build_tabs_strip(&model, &actions, t, cx);
    let tools = build_tools(actions.on_hide, t);

    let root = div()
        .id("terminal-drawer-header")
        .test_support()
        .flex()
        .flex_none()
        .rounded_t(t.radii.card)
        .bg(t.colors.background)
        .border_b_1()
        .border_color(t.colors.line);

    if is_two_row {
        root.flex_col()
            .w_full()
            .child(
                div()
                    .id("terminal-header-row-tabs")
                    .test_support()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .h(px(tokens::HEADER_HEIGHT))
                    .px(t.spacing.sm)
                    .child(tabs_strip),
            )
            .child(
                div()
                    .id("terminal-header-row-tools")
                    .test_support()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.sm)
                    .w_full()
                    .min_w_0()
                    .h(px(tokens::HEADER_HEIGHT))
                    .px(t.spacing.sm)
                    .border_t_1()
                    .border_color(t.colors.line)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(t.typography.small)
                            .text_color(t.colors.muted)
                            .child(summary),
                    )
                    .child(tools),
            )
            .into_any_element()
    } else {
        root.items_center()
            .justify_between()
            .gap(t.spacing.sm)
            .h(px(tokens::HEADER_HEIGHT))
            .px(t.spacing.sm)
            .child(tabs_strip)
            .child(tools)
            .into_any_element()
    }
}

fn build_tabs_strip(
    model: &HeaderModel,
    actions: &HeaderActions,
    t: &Theme,
    cx: &App,
) -> AnyElement {
    let tabs = model.tabs.iter().map(|tab| {
        Tab::new(tab.label.clone())
            .icon(tab.icon)
            .maybe_status(tab.status)
            .italic(tab.italic)
    });
    let on_select = Rc::clone(&actions.on_select);
    let on_close = Rc::clone(&actions.on_close);
    let new_group = build_new_group(model, actions, t, cx);

    Tabs::new("terminal-tabs", tabs)
        .aria_label(navo_l10n::tr("terminal-tabs-label"))
        .menu(model.tabs_menu.clone())
        .selected(model.active)
        .highlighted(model.is_focused)
        .trailing(new_group, px(NEW_GROUP_WIDTH))
        .on_select(move |index, window, cx| on_select(index, window, cx))
        .on_close(move |index, window, cx| on_close(index, window, cx))
        .into_any_element()
}

fn build_new_group(
    model: &HeaderModel,
    actions: &HeaderActions,
    t: &Theme,
    cx: &App,
) -> AnyElement {
    let new_label = SharedString::from(navo_l10n::tr("terminal-new-session"));
    let new_tip = SharedString::from(navo_l10n::tr("terminal-new-session-tip"));
    let picker_label = SharedString::from(navo_l10n::tr("terminal-shell-picker"));
    let picker_tip = SharedString::from(navo_l10n::tr("terminal-shell-picker-tip"));
    let hover = t.colors.hover;
    let is_open = model.shell_picker.read(cx).is_open;
    let on_new = Rc::clone(&actions.on_new);
    let toggle_state = model.shell_picker.clone();

    let add_btn = div()
        .id("terminal-new-session")
        .role(Role::Button)
        .aria_label(new_label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(NEW_BTN_WIDTH))
        .h(t.spacing.tab_height)
        .rounded(t.radii.md)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .tooltip(move |window, cx| Tooltip::new(new_tip.clone()).build(window, cx))
        .on_click(move |_, window, cx| on_new(window, cx))
        .child(icon(IconName::Plus, px(ICON_SIZE), t.colors.text));

    let picker_btn = wire_anchor(
        div()
            .id("terminal-shell-picker")
            .role(Role::Button)
            .aria_label(picker_label)
            .aria_expanded(is_open)
            .test_support(),
        (&model.shell_picker, t.colors.accent),
        cx,
    )
    .flex()
    .flex_none()
    .items_center()
    .justify_center()
    .w(px(PICKER_BTN_WIDTH))
    .h(t.spacing.tab_height)
    .rounded(t.radii.md)
    .cursor_pointer()
    .when(is_open, |d| d.bg(hover))
    .hover(move |style| style.bg(hover))
    .tooltip(move |window, cx| Tooltip::new(picker_tip.clone()).build(window, cx))
    .on_click(move |event, window, cx| toggle(&toggle_state, event, window, cx))
    .child(icon(IconName::ChevronDown, px(12.0), t.colors.muted));

    let menu = is_open.then(|| build_shell_menu(model, Rc::clone(&actions.on_pick_shell), t, cx));

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.0))
        .child(add_btn)
        .child(
            div()
                .relative()
                .flex_none()
                .child(picker_btn)
                .children(menu),
        )
        .into_any_element()
}

fn build_shell_menu(model: &HeaderModel, on_pick: ShellHandler, t: &Theme, cx: &App) -> AnyElement {
    let shells = model.shells.clone();
    let count = shells.len();
    let selected = model.shell_picker.read(cx).selected;
    let on_choose: ChooseHandler = Rc::new(move |position, window, cx| {
        if let Some(shell) = shells.get(position).cloned() {
            on_pick(shell, window, cx);
        }
    });

    let rows: Vec<AnyElement> = model
        .shells
        .iter()
        .enumerate()
        .map(|(index, shell)| {
            let row_props = PickerRowProps {
                index,
                label: SharedString::from(shell.label.clone()),
                is_default: model.default_shell == Some(index),
                is_selected: index == selected,
                state: model.shell_picker.clone(),
                on_choose: Rc::clone(&on_choose),
            };
            picker_row(t, row_props)
        })
        .collect();

    let keys = Keys {
        state: model.shell_picker.clone(),
        count,
        on_choose,
    };
    let panel = wire_keys(div().id("terminal-shell-picker-menu"), keys, cx)
        .role(Role::Menu)
        .aria_label(navo_l10n::tr("terminal-shell-picker-menu"))
        .occlude()
        .flex()
        .flex_col()
        .w(px(PICKER_MENU_WIDTH))
        .p(t.spacing.xs)
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.sheet)
        .shadow(t.shadows.menu.clone())
        .text_color(t.colors.text)
        .text_size(t.typography.small)
        .children(rows);

    let offset = point(px(0.0), t.spacing.tab_height + px(4.0));
    deferred(
        anchored()
            .anchor(Anchor::TopLeft)
            .offset(offset)
            .snap_to_window_with_margin(px(8.0))
            .child(panel),
    )
    .with_priority(1)
    .into_any_element()
}

struct PickerRowProps {
    index: usize,
    label: SharedString,
    is_default: bool,
    is_selected: bool,
    state: Entity<PopupMenu>,
    on_choose: ChooseHandler,
}

fn picker_row(theme: &Theme, props: PickerRowProps) -> AnyElement {
    let resolved = theme.selected_content(props.is_selected);
    let t = &resolved;
    let hover = if props.is_selected && t.is_contrast {
        t.colors.selection
    } else {
        t.colors.hover
    };
    let default_tag = props
        .is_default
        .then(|| SharedString::from(navo_l10n::tr("terminal-shell-default-tag")));
    let index = props.index;
    let state = props.state;
    let on_choose = props.on_choose;

    div()
        .id(("terminal-shell-option", index))
        .role(Role::MenuItem)
        .aria_label(props.label.clone())
        .aria_selected(props.is_selected)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(PICKER_ROW_HEIGHT))
        .px(t.spacing.sm)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(props.is_selected, |d| {
            d.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .hover(move |style| style.bg(hover))
        .on_click(move |_, window, cx| choose(&state, (index, &on_choose), window, cx))
        .child(icon(
            IconName::SquareTerminal,
            px(ICON_SIZE),
            t.colors.muted,
        ))
        .child(div().flex_1().min_w_0().truncate().child(props.label))
        .when_some(default_tag, |d, tag| {
            d.child(
                div()
                    .flex_none()
                    .text_size(t.typography.small)
                    .text_color(t.colors.accent_text)
                    .child(tag),
            )
        })
        .into_any_element()
}

fn build_tools(on_hide: Callback, t: &Theme) -> AnyElement {
    let hide_label = SharedString::from(navo_l10n::tr("terminal-drawer-hide"));
    div()
        .id("terminal-header-tools")
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .child(
            IconButton::new("terminal-drawer-hide", IconName::ChevronDown, hide_label).on_click(
                move |_, window, cx| {
                    on_hide(window, cx);
                },
            ),
        )
        .into_any_element()
}
