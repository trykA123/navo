//! Keyboard focus, context wiring, and navigation handlers for the button bar.

use gpui_kit::Stateful;

use super::{ButtonAction, ButtonItem};
use crate::prelude::*;
use crate::settings::WindowParts;
use crate::window_view::{NavoWindow, keys};

pub(super) const CONTEXT: &str = "ButtonBar";

fn is_button(item: &ButtonItem) -> bool {
    !matches!(item.action, ButtonAction::Separator)
}

fn first_button(items: &[ButtonItem]) -> Option<usize> {
    items.iter().position(is_button)
}

fn step_button(items: &[ButtonItem], current: Option<usize>, forward: bool) -> Option<usize> {
    let Some(index) = current.filter(|&index| index < items.len()) else {
        return if forward {
            first_button(items)
        } else {
            items.iter().rposition(is_button)
        };
    };
    if forward {
        (index + 1..items.len())
            .chain(0..=index)
            .find(|&pos| is_button(&items[pos]))
    } else {
        (0..index)
            .rev()
            .chain((index..items.len()).rev())
            .find(|&pos| is_button(&items[pos]))
    }
}

impl NavoWindow {
    /// Returns whether the button bar strip is currently visible in this window.
    pub(super) fn button_bar_shown(&self, cx: &App) -> bool {
        let parts = WindowParts::of(&self.values);
        parts.button_bar
            && self.viewer.is_none()
            && !self.settings.is_open()
            && !self.host_screen_open(cx)
    }

    /// Attaches focus tracking, the `ButtonBar` key context, and navigation handlers to `bar`.
    pub(super) fn button_bar_keys(
        &self,
        bar: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        bar.track_focus(&self.button_bar.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|view, _: &keys::ButtonBarPrevious, _, cx| {
                view.step_button_bar(false, cx);
            }))
            .on_action(cx.listener(|view, _: &keys::ButtonBarNext, _, cx| {
                view.step_button_bar(true, cx);
            }))
            .on_action(cx.listener(|view, _: &keys::ButtonBarRun, window, cx| {
                view.run_focused_button(window, cx);
            }))
            .on_action(
                cx.listener(|view, _: &keys::ButtonBarOpenMenu, window, cx| {
                    view.open_focused_button_menu(window, cx);
                }),
            )
            .on_action(
                cx.listener(|view, _: &keys::ButtonBarContextMenu, window, cx| {
                    view.open_button_bar_context_menu_at_focus(window, cx);
                }),
            )
            .on_action(cx.listener(|view, _: &keys::ButtonBarLeave, window, cx| {
                view.leave_button_bar(window, cx);
            }))
    }

    /// Attaches the window-level `FocusButtonBar` and `EditButtonBar` handlers to `root`.
    pub(in crate::window_view) fn button_bar_focus_key(
        &self,
        root: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        root.on_action(cx.listener(|view, _: &keys::FocusButtonBar, window, cx| {
            view.focus_button_bar(window, cx);
        }))
        .on_action(cx.listener(|view, _: &keys::EditButtonBar, window, cx| {
            view.open_button_bar_editor(None, window, cx);
        }))
    }

    fn focus_button_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.button_bar_shown(cx) {
            return;
        }
        let Some(index) = first_button(&self.button_bar.items) else {
            return;
        };
        self.button_bar.focused = Some(index);
        let handle = self.button_bar.focus.clone();
        window.focus(&handle, cx);
        cx.notify();
    }

    fn step_button_bar(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.button_bar.focused =
            step_button(&self.button_bar.items, self.button_bar.focused, forward);
        cx.notify();
    }

    fn run_focused_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self
            .button_bar
            .focused
            .and_then(|index| self.button_bar.items.get(index))
            .cloned()
        else {
            return;
        };
        self.run_button(&item, None, window, cx);
    }

    fn open_focused_button_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.button_bar.focused else {
            return;
        };
        let Some(item) = self.button_bar.items.get(index).cloned() else {
            return;
        };
        if let ButtonAction::Menu(children) = &item.action {
            self.open_button_menu_at(Some(index), &item.name, children, window, cx);
        }
    }

    fn leave_button_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.button_bar.focused = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}
