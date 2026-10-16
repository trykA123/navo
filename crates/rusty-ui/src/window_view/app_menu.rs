use std::rc::Rc;

use gpui_kit::FocusHandle;

use super::{FerriteWindow, mode_of};
use crate::app_menu::{AppMenu, AppMenuEvent, Identity, Install, MenuCommand, Setting};
use crate::prelude::*;
use crate::tokens::{self, ThemeChoice};

pub type SaveTheme = Rc<dyn Fn(ThemeChoice)>;

pub struct AppMenuHooks {
    pub identity: Identity,
    pub theme: ThemeChoice,
    pub save_theme: SaveTheme,
}

impl AppMenuHooks {
    pub fn none() -> Self {
        Self {
            identity: Identity {
                version: SharedString::default(),
                install: Install::Installed,
                folder: SharedString::default(),
            },
            theme: ThemeChoice::System,
            save_theme: Rc::new(|_| {}),
        }
    }
}

pub(super) struct OpenMenu {
    selected: usize,
    restore: Option<FocusHandle>,
}

impl FerriteWindow {
    pub(super) fn apply_theme(&self, window: &Window, cx: &mut App) {
        tokens::apply(self.theme.resolve(mode_of(window.appearance())), cx);
    }

    pub(super) fn toggle_app_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
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
            AppMenuEvent::Toggle(Setting::Theme) => {
                self.theme = self.theme.next();
                (self.save_theme)(self.theme);
                self.apply_theme(window, cx);
            }
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
        let view = AppMenu::new("app-menu", self.identity.clone(), self.theme)
            .selected(menu.selected)
            .track_focus(&self.app_menu_focus)
            .on_event(
                cx.listener(|this: &mut Self, event: &AppMenuEvent, window, cx| {
                    this.app_menu_event(*event, window, cx);
                }),
            );
        Some(view.into_overlay(&t))
    }
}
