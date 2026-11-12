//! The window title bar: the search field and the trailing "View" button, put together into
//! the `NavoTitleBar` drawn at the top of every window.

use gpui_kit::Action;
use gpui_kit::canvas;

use super::NavoWindow;
use super::keys::OpenViewMenu;
use crate::commands::compare::ViewMenu as CompareViewMenu;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::settings::WindowParts;
use crate::title_bar::{HeaderSearch, NavoTitleBar};

/// The title-bar "View" button. `compare_open` is true while the compare screen is up: the
/// click then runs the compare View menu with "Wrap lines" (W8n), else the window's, as today.
fn view_button(compare_open: bool) -> impl IntoElement {
    Button::new("window-view", "View")
        .variant(ButtonVariant::Ghost)
        .tooltip("View menu (Alt+V)")
        .on_click(move |_, window, cx| {
            let action: Box<dyn Action> = if compare_open {
                Box::new(CompareViewMenu)
            } else {
                Box::new(OpenViewMenu)
            };
            window.dispatch_action(action, cx);
        })
}

impl NavoWindow {
    fn search_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let anchor = self.command_center.anchor.clone();
        let open = cx.listener(|view: &mut Self, _: &ClickEvent, window, cx| {
            if view.command_center_view().is_none() {
                view.toggle_command_center(window, cx);
            }
        });
        div()
            .id("title-bar-search-open")
            .relative()
            .flex()
            .min_w_0()
            .cursor_pointer()
            .on_click(open)
            .child(HeaderSearch::new())
            .child(
                canvas(
                    move |bounds, _, _| anchor.set(Some(bounds)),
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    pub(super) fn search_anchor(&self, cx: &App) -> Option<Bounds<Pixels>> {
        let is_ribbon = self.workspace_ribbon(cx).is_some();
        self.command_center.anchor.get().filter(|_| !is_ribbon)
    }

    pub(super) fn title_bar(&self, cx: &mut Context<Self>) -> NavoTitleBar {
        let this = cx.weak_entity();
        let toggle = cx.listener(|view: &mut Self, _: &ClickEvent, window, cx| {
            view.toggle_app_menu(window, cx);
        });
        let bar = NavoTitleBar::new(navo_core::brand::NAME)
            .on_mica()
            .app_button(self.app_menu.is_some(), toggle);
        let bar = if let Some(ribbon) = self.workspace_ribbon(cx) {
            self.command_center.anchor.set(None);
            bar.header(ribbon)
                .header_inset(crate::tokens::current(cx).spacing.xxl * 8.)
        } else if WindowParts::of(&self.values).title_search {
            bar.header(self.search_field(cx))
        } else {
            self.command_center.anchor.set(None);
            bar
        };
        let bar = if self.can_open_window_menu() {
            bar.trailing(view_button(self.compare.screen.is_some()))
        } else {
            bar
        };
        bar.on_close(move |window, cx| {
            if this
                .update(cx, |view, cx| view.close_or_hide(window, cx))
                .unwrap_or(true)
            {
                cx.quit();
            }
        })
    }
}
