use gpui_kit::FocusHandle;

use super::{FerriteWindow, mode_of};
use crate::app_menu::{AppMenu, AppMenuEvent, Identity, Install, MenuCommand};
use crate::prelude::*;
use crate::tokens;

pub struct AppMenuHooks {
    pub identity: Identity,
}

impl AppMenuHooks {
    pub fn none() -> Self {
        Self {
            identity: Identity {
                version: SharedString::default(),
                install: Install::Installed,
                folder: SharedString::default(),
            },
        }
    }
}

pub(super) struct OpenMenu {
    selected: usize,
    restore: Option<FocusHandle>,
}

impl FerriteWindow {
    pub(super) fn apply_theme(&self, window: &Window, cx: &mut App) {
        let mode = self.values.theme().resolve(mode_of(window.appearance()));
        tokens::apply_look(mode, self.values.density(), self.values.syntax_theme(), cx);
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

    pub(super) fn close_app_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                self.run_menu_command(command, window, cx);
            }
            AppMenuEvent::Close => self.close_app_menu(window, cx),
        }
        cx.notify();
    }

    fn run_menu_command(
        &mut self,
        command: MenuCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            MenuCommand::Quit if self.close(window, cx) => cx.quit(),
            MenuCommand::AllSettings => self.open_settings(window, cx),
            MenuCommand::CommandPalette => self.toggle_command_center(window, cx),
            _ => {}
        }
    }

    pub(super) fn app_menu_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.app_menu.as_ref()?;
        let t = tokens::current(cx).clone();
        let view = AppMenu::new("app-menu", self.identity.clone(), self.values.quick())
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
