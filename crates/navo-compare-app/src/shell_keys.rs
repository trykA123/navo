//! Keyboard wiring of the shell: its actions, key bindings, F6 pane cycling and hover-tip Escape.
//! A child module of `shell`, so it reads the shell's private fields.

use gpui_kit::{App, Context, Div, FocusHandle, InteractiveElement as _, KeyBinding, Window};

use super::Shell;
use crate::chrome::sidebar::{self, DismissTip, ToggleRail};
use crate::chrome::topbar_status;
use crate::layers::keys_sheet;
use crate::screens::compare_tab::OWN;
use crate::screens::{compare_tab_keys, folders_view, large_view};
use crate::start;

gpui_kit::actions!(
    shell,
    [
        NewTab, CloseTab, NextTab, PrevTab, CyclePane, ThemeMenu, RefreshTab
    ]
);

/// Key context of the shell's root element; the tab keys are bound inside it.
pub(super) const CONTEXT: &str = "Shell";

/// Tracks `handle` on the root element and adds the shell-level F5 handler, bound inside the
/// topbar filter.
pub(super) fn wire_refresh(root: Div, handle: &FocusHandle, cx: &mut Context<Shell>) -> Div {
    root.track_focus(handle)
        .on_action(cx.listener(|this, _: &RefreshTab, window, cx| this.refresh_active(window, cx)))
}

/// Binds the shell, start screen, sidebar, and topbar keys; call once after `sidewise_kit::init`.
pub fn bind_keys(cx: &mut App) {
    start::bind_keys(cx);
    folders_view::bind_keys(cx);
    large_view::bind_keys(cx);
    compare_tab_keys::bind_keys(cx);
    sidebar::bind_keys(cx);
    topbar_status::bind_keys(cx);
    keys_sheet::bind_keys(cx);
    crate::menu_host::bind_menu_keys(cx);
    cx.bind_keys([
        KeyBinding::new("ctrl-t", NewTab, Some(CONTEXT)),
        KeyBinding::new("1", NewTab, Some(OWN)),
        KeyBinding::new("ctrl-w", CloseTab, Some(CONTEXT)),
        KeyBinding::new("ctrl-tab", NextTab, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PrevTab, Some(CONTEXT)),
        KeyBinding::new("ctrl-\\", ToggleRail, Some(CONTEXT)),
        KeyBinding::new("f6", CyclePane, Some(CONTEXT)),
        KeyBinding::new("shift-f6", CyclePane, Some(CONTEXT)),
        KeyBinding::new("escape", DismissTip, Some(CONTEXT)),
        KeyBinding::new("alt-v", topbar_status::ViewMenu, Some(CONTEXT)),
        KeyBinding::new("alt-t", ThemeMenu, Some(CONTEXT)),
        // The filter field sits in the topbar, outside the tab's own F5 binding.
        KeyBinding::new("f5", RefreshTab, Some("Topbar > Input")),
    ]);
}

impl Shell {
    /// F5 from the topbar filter: compares the active compare tab again; a Start tab ignores it.
    fn refresh_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_compare().cloned() {
            tab.update(cx, |tab, cx| tab.refresh(window, cx));
        }
    }

    /// The topbar filter text, for tests.
    #[cfg(test)]
    pub(crate) fn topbar_filter(&self, cx: &App) -> String {
        self.topbar.read(cx).filter().to_string()
    }

    /// F6 / Shift+F6: moves focus to the other of the two panes, the sidebar or the active start
    /// screen. Does not resync tab data, so an overlay-only hover tip stays.
    pub(super) fn cycle_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.read(cx).contains_focus(window, cx) {
            self.activated(window, cx);
        } else {
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.focus_primary(window, cx));
            cx.notify();
        }
    }

    /// `Escape` from Main: dismisses a shown hover tip through the sidebar, else passes on.
    pub(super) fn dismiss_tip(&mut self, cx: &mut Context<Self>) {
        if !self
            .sidebar
            .update(cx, |sidebar, cx| sidebar.dismiss_tip(cx))
        {
            cx.propagate();
        }
    }
}
