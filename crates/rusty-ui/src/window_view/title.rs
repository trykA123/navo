use super::FerriteWindow;
use super::keys::OpenViewMenu;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::settings::WindowParts;
use crate::title_bar::{FerriteTitleBar, HeaderSearch};

fn view_button() -> impl IntoElement {
    Button::new("window-view", "View")
        .variant(ButtonVariant::Ghost)
        .tooltip("View menu (Alt+V)")
        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenViewMenu), cx))
}

impl FerriteWindow {
    pub(super) fn title_bar(&self, cx: &mut Context<Self>) -> FerriteTitleBar {
        let this = cx.weak_entity();
        let toggle = cx.listener(|view: &mut Self, _: &ClickEvent, window, cx| {
            view.toggle_app_menu(window, cx);
        });
        let bar = FerriteTitleBar::new("Ferrite")
            .on_mica()
            .app_button(self.app_menu.is_some(), toggle);
        let bar = if WindowParts::of(&self.values).title_search {
            bar.header(HeaderSearch::new())
        } else {
            bar
        };
        let bar = if self.can_open_window_menu() {
            bar.trailing(view_button())
        } else {
            bar
        };
        bar.on_close(move |window, cx| {
            if this
                .update(cx, |view, cx| view.close(window, cx))
                .unwrap_or(true)
            {
                cx.quit();
            }
        })
    }
}
