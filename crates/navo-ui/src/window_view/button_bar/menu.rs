//! Button bar right-click context menu and `ButtonAction::Menu` button popups.

use std::rc::Rc;

use gpui_kit::{
    AnchoredPositionMode, AnyElement, ClickEvent, FocusHandle, Role, anchored, deferred, point, px,
};
use navo_core::shell_menu::{Group, ItemId, MenuItem, ShellMenu};
use navo_l10n::tr;

use super::fit;
use super::view::{PopupRowProps, popup_row, tooltip_text};
use super::{
    BarEdit, ButtonAction, ButtonItem, ItemPath, MSG_BAR_LABEL, MSG_HIDE, MSG_REMOVE_BUTTON,
    MSG_SHOW_NAMES,
};
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::primitives::popup_menu::{self, ChooseHandler, Keys, PopupMenu, wire_keys};
use crate::settings::{Change, Field, Value};
use crate::status_bar::StatusNotice;
use crate::tokens;
use crate::window_view::NavoWindow;

const ACT_REMOVE: &str = "button_bar.remove";
const ACT_SHOW_NAMES: &str = "button_bar.show_names";
const ACT_HIDE: &str = "button_bar.hide";
const POPUP_WIDTH: f32 = 240.;
const POPUP_ROW_HEIGHT: f32 = 30.;
const POPUP_GAP: f32 = 4.;
const POPUP_MARGIN: f32 = 8.;

/// Prepainted bar and button bounds used to anchor keyboard-opened menus.
#[derive(Default)]
pub(super) struct BarBounds {
    pub(super) bar: Option<Bounds<Pixels>>,
    pub(super) buttons: Vec<Bounds<Pixels>>,
}

impl BarBounds {
    pub(super) fn anchor(&self, index: Option<usize>) -> Point<Pixels> {
        if let Some(bounds) = index.and_then(|i| self.buttons.get(i)) {
            return point(bounds.origin.x, bounds.bottom());
        }
        if let Some(bounds) = self.bar {
            return point(bounds.origin.x + px(POPUP_MARGIN), bounds.bottom());
        }
        point(px(POPUP_MARGIN), px(POPUP_ROW_HEIGHT))
    }
}

/// Active child popup opened from a `ButtonAction::Menu` button.
pub(super) struct OpenButtonMenu {
    pub(super) index: Option<usize>,
    pub(super) label: String,
    pub(super) items: Vec<ButtonItem>,
    pub(super) anchor: Point<Pixels>,
}

/// Builds the `ShellMenu` for the button bar context menu.
#[must_use]
pub(crate) fn bar_context_menu(has_button: bool, show_names: bool) -> ShellMenu {
    let mut items = Vec::new();
    if has_button {
        items.push(MenuItem::new(
            ItemId::Action(ACT_REMOVE),
            &tr(MSG_REMOVE_BUTTON),
            Group::Navo,
        ));
        items.push(MenuItem::separator(Group::Navo));
    }
    items.push(MenuItem {
        checked: show_names,
        ..MenuItem::new(
            ItemId::Action(ACT_SHOW_NAMES),
            &tr(MSG_SHOW_NAMES),
            Group::Navo,
        )
    });
    items.push(MenuItem::new(
        ItemId::Action(ACT_HIDE),
        &tr(MSG_HIDE),
        Group::Navo,
    ));
    ShellMenu { items }
}

impl NavoWindow {
    /// Returns the open button bar context menu entity for window rendering.
    pub(in crate::window_view) fn button_bar_context_menu_view(
        &self,
    ) -> Option<Entity<ContextMenu>> {
        self.button_bar
            .context_menu
            .as_ref()
            .map(|(menu, _)| menu.clone())
    }

    /// Opens the button bar context menu at `anchor`, optionally targeting button `index`.
    pub(super) fn open_button_bar_context_menu(
        &mut self,
        index: Option<usize>,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.button_bar_shown(cx) {
            return;
        }
        self.close_popups(window, cx);
        let target = index.filter(|&i| i < self.button_bar.items.len());
        let restore = window
            .focused(cx)
            .unwrap_or_else(|| self.handle(self.browser.state().focused_pane(), cx));
        let show_names = self.values.flag(Field::WindowButtonBarNames);
        let input = ContextMenuInput {
            menu: bar_context_menu(target.is_some(), show_names),
            context: tr(MSG_BAR_LABEL).into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(restore),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, move |this, _, event, window, cx| {
            this.button_bar_context_menu_event(target, event, window, cx);
        });
        self.button_bar.context_menu = Some((menu, events));
        cx.notify();
    }

    /// Opens the button bar context menu anchored to the currently focused bar button.
    pub(super) fn open_button_bar_context_menu_at_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .button_bar
            .focused
            .filter(|&i| i < self.button_bar.items.len());
        let anchor = self.button_bar.bounds.borrow().anchor(target);
        self.open_button_bar_context_menu(target, anchor, window, cx);
    }

    fn button_bar_context_menu_event(
        &mut self,
        target: Option<usize>,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
            return;
        }
        self.button_bar.context_menu = None;
        if let ContextMenuEvent::Chosen(ItemId::Action(name)) = event {
            self.run_button_bar_context_action(target, name, window, cx);
        }
        cx.notify();
    }

    fn run_button_bar_context_action(
        &mut self,
        target: Option<usize>,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ACT_REMOVE => {
                if let Some(index) = target {
                    self.remove_button_bar_item(index, cx);
                }
            }
            ACT_SHOW_NAMES => {
                let next = !self.values.flag(Field::WindowButtonBarNames);
                self.change_setting(
                    Change {
                        field: Field::WindowButtonBarNames,
                        value: Value::Flag(next),
                    },
                    window,
                    cx,
                );
            }
            ACT_HIDE => {
                self.button_bar.focused = None;
                self.change_setting(
                    Change {
                        field: Field::WindowButtonBar,
                        value: Value::Flag(false),
                    },
                    window,
                    cx,
                );
                self.focus_core_pane(window, cx);
            }
            _ => {}
        }
    }

    fn remove_button_bar_item(&self, index: usize, cx: &mut Context<Self>) {
        let Some(edit) = self
            .button_bar
            .hooks
            .as_ref()
            .map(|hooks| hooks.edit.clone())
        else {
            return;
        };
        let request = BarEdit::Remove {
            at: ItemPath { index, child: None },
        };
        let work = cx.background_executor().spawn(async move { edit(request) });
        cx.spawn(async move |this, cx| {
            let Err(message) = work.await else {
                return;
            };
            let _ = this.update(cx, |view, cx| view.note_button_bar_error(message, cx));
        })
        .detach();
    }

    fn note_button_bar_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.button_bar.notice = Some(StatusNotice {
            text: message.into(),
            is_error: true,
        });
        cx.notify();
    }

    /// Handles clicking a visible top-level bar button at `index`.
    pub(super) fn click_bar_button(
        &mut self,
        index: usize,
        item: &ButtonItem,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ButtonAction::Menu(children) = &item.action {
            let dismissed = self
                .button_bar
                .menu_popup
                .update(cx, |menu, _| menu.take_dismissed(event));
            if dismissed {
                return;
            }
            self.open_button_menu_at(Some(index), &item.name, children, window, cx);
            return;
        }
        self.run_button(item, None, window, cx);
    }

    /// Opens the popup for `item`'s menu `children`, resolving its bar index if visible.
    pub(super) fn open_button_menu(
        &mut self,
        item: &ButtonItem,
        children: &[ButtonItem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = self
            .button_bar
            .focused
            .filter(|&i| self.button_bar.items.get(i) == Some(item));
        let index =
            focused.or_else(|| self.button_bar.items.iter().position(|entry| entry == item));
        self.open_button_menu_at(index, &item.name, children, window, cx);
    }

    /// Opens the child popup for a `ButtonAction::Menu` item anchored at `index`.
    pub(super) fn open_button_menu_at(
        &mut self,
        index: Option<usize>,
        label: &str,
        children: &[ButtonItem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.button_bar_shown(cx) || children.is_empty() {
            return;
        }
        if self.button_bar.menu_popup.read(cx).is_open
            && self.button_bar.open_menu.as_ref().map(|open| open.index) == Some(index)
        {
            self.close_button_menu_popup(window, cx);
            return;
        }
        self.close_popups(window, cx);
        let anchor = self.button_bar.bounds.borrow().anchor(index);
        self.button_bar.open_menu = Some(OpenButtonMenu {
            index,
            label: label.to_owned(),
            items: children.to_vec(),
            anchor,
        });
        popup_menu::open(&self.button_bar.menu_popup, window, cx);
        cx.notify();
    }

    /// Closes any open button bar context menu or menu-button popup.
    pub(in crate::window_view) fn close_button_bar_popups(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.context_menu.take().is_some() {
            cx.notify();
        }
        self.close_button_menu_popup(window, cx);
    }

    /// Closes the active menu-button popup, restoring focus if held.
    pub(in crate::window_view) fn close_button_menu_popup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.button_bar.open_menu.take().is_some() {
            popup_menu::close(&self.button_bar.menu_popup, window, cx);
            cx.notify();
        }
    }

    /// Returns whether `focused` belongs to the open menu-button popup.
    pub(in crate::window_view) fn button_bar_menu_holds(
        &self,
        focused: &FocusHandle,
        cx: &App,
    ) -> bool {
        self.button_bar.menu_popup.read(cx).holds(focused)
    }

    /// Renders the active `ButtonAction::Menu` popup overlay when open.
    pub(in crate::window_view) fn button_menu_popup_view(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.button_bar_shown(cx) {
            return None;
        }
        let open = self.button_bar.open_menu.as_ref()?;
        let popup = &self.button_bar.menu_popup;
        if !popup.read(cx).is_open {
            return None;
        }
        let t = tokens::current(cx);
        let runnable_children: Vec<ButtonItem> = open
            .items
            .iter()
            .filter(|item| !matches!(item.action, ButtonAction::Separator))
            .cloned()
            .collect();
        let view = cx.entity();
        let on_choose: ChooseHandler = Rc::new(move |position, window, cx| {
            let Some(item) = runnable_children.get(position) else {
                return;
            };
            view.update(cx, |view, cx| view.run_menu_child(item, window, cx));
        });
        let (rows, count) = self.build_menu_rows(open, popup, &on_choose, cx);
        let keys = Keys {
            state: popup.clone(),
            count,
            on_choose,
        };
        let panel = wire_keys(div().id("button-bar-menu-popup"), keys, cx)
            .role(Role::Menu)
            .aria_label(open.label.clone())
            .occlude()
            .flex()
            .flex_col()
            .w(px(POPUP_WIDTH))
            .p(t.spacing.xs)
            .rounded(t.radii.md)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .shadow(t.shadows.menu.clone())
            .text_color(t.colors.text)
            .text_size(t.typography.small)
            .children(rows);
        let placed = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(open.anchor)
            .offset(point(px(0.), px(POPUP_GAP)))
            .snap_to_window_with_margin(px(POPUP_MARGIN))
            .child(panel);
        Some(deferred(placed).with_priority(2).into_any_element())
    }

    fn run_menu_child(&mut self, item: &ButtonItem, window: &mut Window, cx: &mut Context<Self>) {
        self.button_bar.open_menu = None;
        self.run_button(item, None, window, cx);
    }

    fn build_menu_rows(
        &self,
        open: &OpenButtonMenu,
        popup: &Entity<PopupMenu>,
        on_choose: &ChooseHandler,
        cx: &App,
    ) -> (Vec<AnyElement>, usize) {
        let t = tokens::current(cx);
        let metrics = fit::metrics(&t.spacing, t.density);
        let selected = popup.read(cx).selected;
        let mut position = 0usize;
        let rows = open
            .items
            .iter()
            .map(|item| {
                if matches!(item.action, ButtonAction::Separator) {
                    return div()
                        .h(px(1.))
                        .my(t.spacing.xxs)
                        .bg(t.colors.line)
                        .into_any_element();
                }
                let row_pos = position;
                position += 1;
                let tip = tooltip_text(item, self.user_command_list());
                popup_row(
                    item,
                    tip,
                    PopupRowProps {
                        id: ("button-bar-menu-row", row_pos),
                        theme: t,
                        icon_size: metrics.icon,
                        position: row_pos,
                        is_selected: row_pos == selected,
                        popup: popup.clone(),
                        on_choose: on_choose.clone(),
                    },
                    cx,
                )
            })
            .collect();
        (rows, position)
    }
}
