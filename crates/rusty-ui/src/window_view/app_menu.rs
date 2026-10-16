use std::sync::Arc;

use gpui_kit::FocusHandle;

use super::{FerriteWindow, mode_of};
use crate::app_menu::{AppMenu, AppMenuEvent, Identity, Install, MenuCommand, QuickSettings};
use crate::prelude::*;
use crate::tokens;

pub use super::settings_save::SaveSettings;

pub struct AppMenuHooks {
    pub identity: Identity,
    pub settings: QuickSettings,
    pub load_error: Option<SharedString>,
    pub save: SaveSettings,
}

impl AppMenuHooks {
    pub fn none() -> Self {
        Self {
            identity: Identity {
                version: SharedString::default(),
                install: Install::Installed,
                folder: SharedString::default(),
            },
            settings: QuickSettings {
                show_hidden: true,
                ..QuickSettings::default()
            },
            load_error: None,
            save: Arc::new(|change| {
                let values = QuickSettings {
                    show_hidden: true,
                    ..QuickSettings::default()
                };
                Ok(change.applied_to(values))
            }),
        }
    }
}

pub(super) struct OpenMenu {
    selected: usize,
    restore: Option<FocusHandle>,
}

impl FerriteWindow {
    pub(super) fn apply_theme(&self, window: &Window, cx: &mut App) {
        let mode = self.quick.theme.resolve(mode_of(window.appearance()));
        tokens::apply_with(mode, self.quick.density, cx);
    }

    pub(super) fn toggle_app_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
            return;
        }
        if self.prompt.is_some() || self.browser.pending().is_some() {
            return;
        }
        self.app_menu = Some(OpenMenu {
            selected: 0,
            restore: window.focused(cx),
        });
        window.focus(&self.app_menu_focus, cx);
        cx.notify();
    }

    fn close_app_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = self.app_menu.take().and_then(|menu| menu.restore);
        match restore {
            Some(handle) => window.focus(&handle, cx),
            None => self.focus_core_pane(window, cx),
        }
        cx.notify();
    }

    fn app_menu_event(&mut self, event: AppMenuEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            AppMenuEvent::Select(index) => {
                if let Some(menu) = &mut self.app_menu {
                    menu.selected = index;
                }
            }
            AppMenuEvent::Toggle(setting) => self.toggle_setting(setting, window, cx),
            AppMenuEvent::Run(command) => {
                self.close_app_menu(window, cx);
                self.run_menu_command(command, cx);
            }
            AppMenuEvent::Close => self.close_app_menu(window, cx),
        }
        cx.notify();
    }

    fn run_menu_command(&mut self, command: MenuCommand, cx: &mut Context<Self>) {
        if command == MenuCommand::Quit && self.close(cx) {
            cx.quit();
        }
    }

    pub(super) fn app_menu_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.app_menu.as_ref()?;
        let t = tokens::current(cx).clone();
        let view = AppMenu::new("app-menu", self.identity.clone(), self.quick)
            .selected(menu.selected)
            .error(self.saver.error.clone())
            .track_focus(&self.app_menu_focus)
            .on_event(
                cx.listener(|this: &mut Self, event: &AppMenuEvent, window, cx| {
                    this.app_menu_event(*event, window, cx);
                }),
            );
        Some(view.into_overlay(&t))
    }
}
