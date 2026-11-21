//! The shell's Theme picker host (NC152, NC153): opens the picker, saves a pick, and applies the
//! palette and mode through the rate gate. A child of `shell_layers`, so it reads the shell's fields.

use gpui_kit::{
    AnyElement, AnyView, AnyWindowHandle, App, MouseButton, MouseDownEvent, Subscription, Task,
    WeakEntity, deferred,
};
use sidewise_kit::prelude::*;

use super::Shell;
use crate::chrome::topbar_status::Action;
use crate::layers::theme_menu::{ThemeMenu, ThemeMenuEvent};
use crate::shell_prefs;
use crate::theme::gate::{GateStep, ThemeGate};

/// The Theme picker while open, the rate gate for applied changes, and the one trailing apply.
#[derive(Default)]
pub(super) struct ThemeLayer {
    menu: Option<(Entity<ThemeMenu>, Subscription)>,
    gate: ThemeGate,
    trailing: Option<Task<()>>,
}

/// The trailing apply: marks the gate fired, then applies the latest saved choice.
fn fire_trailing(handle: AnyWindowHandle, shell: WeakEntity<Shell>, cx: &mut App) {
    let _ = handle.update(cx, |_, window, cx| {
        let _ = shell.update(cx, |shell, cx| {
            let now = cx.background_executor().now();
            shell.layers.theme.gate.fired(now);
            shell.apply_theme(window, cx);
        });
    });
}

impl Shell {
    /// Theme pill or Alt+T: opens the picker, or closes it and refocuses the tab.
    pub(in crate::shell) fn toggle_theme_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layers.theme.menu.is_some() {
            self.set_theme_menu(None, cx);
            self.activated(window, cx);
        } else {
            self.open_theme_menu(window, cx);
        }
    }

    /// Closes the View menu, opens the picker on the saved palette and mode, focuses it.
    fn open_theme_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_view_menu(None, cx);
        let prefs = shell_prefs::prefs(cx);
        let menu = cx.new(|cx| ThemeMenu::new(prefs.palette, prefs.mode, cx));
        let sub = cx.subscribe_in(&menu, window, |this, menu, event, window, cx| match event {
            ThemeMenuEvent::Changed => this.on_theme_pick(menu, window, cx),
            ThemeMenuEvent::Dismissed => {
                this.set_theme_menu(None, cx);
                this.activated(window, cx);
            }
        });
        menu.update(cx, |menu, cx| menu.focus(window, cx));
        self.set_theme_menu(Some((menu, sub)), cx);
    }

    /// While the picker is open: a full-window layer under it that swallows a mouse-down outside
    /// the card, closes the picker and returns focus to the tab, as the View menu does.
    pub(in crate::shell) fn theme_backdrop(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.layers.theme.menu.as_ref()?;
        let dismiss =
            |this: &mut Self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>| {
                this.set_theme_menu(None, cx);
                this.activated(window, cx);
            };
        let layer = div()
            .id("theme-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
            .on_mouse_down(MouseButton::Right, cx.listener(dismiss));
        Some(deferred(layer).into_any_element())
    }

    /// Stores the picker (or None) and hands it to the topbar as `(Action::Theme, view)`.
    pub(super) fn set_theme_menu(
        &mut self,
        menu: Option<(Entity<ThemeMenu>, Subscription)>,
        cx: &mut Context<Self>,
    ) {
        let shown = menu
            .as_ref()
            .map(|(menu, _)| (Action::Theme, AnyView::from(menu.clone())));
        self.topbar.update(cx, |bar, cx| bar.set_menu(shown, cx));
        self.layers.theme.menu = menu;
        cx.notify();
    }

    /// Saves the pick into the current prefs (palette and mode only), toasts a save error, then
    /// requests an apply.
    fn on_theme_pick(
        &mut self,
        menu: &Entity<ThemeMenu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (palette, mode) = menu.read(cx).choice();
        let mut prefs = shell_prefs::prefs(cx);
        prefs.palette = palette;
        prefs.mode = mode;
        if let Err(message) = shell_prefs::save_prefs(prefs, cx) {
            self.toasts
                .update(cx, |toasts, cx| toasts.push(message, cx));
        }
        self.request_theme(window, cx);
    }

    /// Runs the gate: Now applies; After(d) spawns one trailing apply that reads the latest
    /// saved choice; Waiting leaves the scheduled apply to it (NC153).
    fn request_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        match self.layers.theme.gate.request(now) {
            GateStep::Now => self.apply_theme(window, cx),
            GateStep::Waiting => {}
            GateStep::After(delay) => {
                let (handle, shell) = (window.window_handle(), cx.weak_entity());
                let timer = cx.background_executor().timer(delay);
                self.layers.theme.trailing = Some(cx.spawn(async move |_, cx| {
                    timer.await;
                    cx.update(|cx| fire_trailing(handle, shell, cx));
                }));
            }
        }
    }

    /// Applies the saved palette and mode in one step and redraws.
    fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        Self::apply_appearance(window.appearance(), cx);
        cx.notify();
    }

    /// Whether the Theme picker shows; for tests.
    #[cfg(test)]
    pub(crate) fn theme_open(&self) -> bool {
        self.layers.theme.menu.is_some()
    }
}

#[cfg(test)]
#[path = "shell_theme_tests.rs"]
mod tests;
