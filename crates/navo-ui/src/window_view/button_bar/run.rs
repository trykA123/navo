//! Dispatches button bar clicks and drops to Navo actions, user commands, and inline programs.

use gpui_kit::{Context, Window};
use navo_core::Location;

use super::view::runnable;
use super::{ButtonAction, ButtonItem, MSG_UNKNOWN_COMMAND};
use crate::status_bar::StatusNotice;
use crate::window_view::NavoWindow;

impl NavoWindow {
    /// Executes `item`, substituting `dropped` files into the invocation when present.
    pub(crate) fn run_button(
        &mut self,
        item: &ButtonItem,
        dropped: Option<Vec<Location>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &item.action {
            ButtonAction::Navo(id) => {
                if let Some(action) = runnable(id) {
                    self.run_action(action, window, cx);
                } else {
                    self.button_bar.notice = Some(StatusNotice {
                        text: navo_l10n::tr(MSG_UNKNOWN_COMMAND).into(),
                        is_error: true,
                    });
                    cx.notify();
                }
            }
            ButtonAction::User(name) => {
                if dropped.is_none() {
                    self.run_user_command(name, cx);
                } else {
                    self.run_user_command_with(name, dropped, cx);
                }
            }
            ButtonAction::Program { id, .. } => {
                let Some(invocation) = self.user_invocation_with_drop(dropped) else {
                    self.refuse_on_parent(&item.name, cx);
                    return;
                };
                if let Some(hooks) = &self.button_bar.hooks {
                    (hooks.run_program)(*id, invocation);
                }
            }
            ButtonAction::Menu(children) => {
                if dropped.is_none() {
                    self.open_button_menu(item, children, window, cx);
                }
            }
            ButtonAction::Separator => {}
        }
    }
}
