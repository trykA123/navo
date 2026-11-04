use std::sync::Arc;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use navo_core::{Location, Tab};
use navo_l10n::t;

use super::{NavoWindow, rebind_app_keys};
use crate::commands::focus_paths::screen_reach;
use crate::commands::{
    Keymap, Reach, RunUserCommand, active, normal_key, set_user_keys, shown, user_keys,
};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

/// A user command as the window lists it: its name and its key text, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserCommandInfo {
    pub name: String,
    pub key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserCommandNews {
    /// A valid file: the new list and the load warnings.
    Applied(Vec<UserCommandInfo>, Vec<String>),
    /// An invalid file: the old list stays and the message shows.
    Invalid(String),
    /// A command ended; `code` is `None` when it is unknown. Only a hidden
    /// console reports a non-zero code; closing a shown one is not a failure.
    Exited {
        name: String,
        code: Option<i32>,
        hidden: bool,
    },
    Refused {
        name: String,
        message: String,
    },
}

/// The pane values a run may use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserInvocation {
    pub file: Option<Location>,
    pub marked: Vec<Location>,
    pub folder: Option<Location>,
    pub other_folder: Option<Location>,
    pub other_file: Option<Location>,
}

/// Starts the named command; refusals and exits come back as news.
pub type UserCommandRun = Arc<dyn Fn(&str, UserInvocation) + Send + Sync>;

/// Writes the commented template when the command file is absent.
pub type UserCommandEdit = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[derive(Clone, Debug)]
pub struct UserCommandSender(UnboundedSender<UserCommandNews>);

impl UserCommandSender {
    pub fn send(&self, news: UserCommandNews) -> bool {
        self.0.unbounded_send(news).is_ok()
    }
}

pub struct UserCommandHooks {
    pub run: UserCommandRun,
    pub edit: UserCommandEdit,
    pub file: Option<Location>,
    pub news: Option<UnboundedReceiver<UserCommandNews>>,
}

impl UserCommandHooks {
    #[must_use]
    pub fn channel(run: UserCommandRun, file: Option<Location>) -> (Self, UserCommandSender) {
        let (sender, receiver) = unbounded();
        let hooks = Self {
            run,
            edit: Arc::new(|| Ok(())),
            file,
            news: Some(receiver),
        };
        (hooks, UserCommandSender(sender))
    }

    #[must_use]
    pub fn with_edit(self, edit: UserCommandEdit) -> Self {
        Self { edit, ..self }
    }
}

#[derive(Default)]
pub(super) struct UserCommandState {
    list: Vec<UserCommandInfo>,
    /// Load problems and warnings; they stay until the next news of the file.
    notice: Option<StatusNotice>,
    /// A refusal or an exit error; the next command forgets it.
    run_notice: Option<StatusNotice>,
    run: Option<UserCommandRun>,
    edit: Option<UserCommandEdit>,
    file: Option<Location>,
}

impl NavoWindow {
    pub(super) fn listen_user_commands(
        &mut self,
        hooks: Option<UserCommandHooks>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(hooks) = hooks else {
            return;
        };
        self.user_commands.run = Some(hooks.run);
        self.user_commands.edit = Some(hooks.edit);
        self.user_commands.file = hooks.file;
        let Some(mut receiver) = hooks.news else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            while let Some(news) = receiver.next().await
                && this
                    .update_in(cx, |view, window, cx| {
                        view.take_user_commands(news, window, cx);
                    })
                    .is_ok()
            {}
        })
        .detach();
    }

    fn take_user_commands(
        &mut self,
        news: UserCommandNews,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.user_commands;
        match news {
            UserCommandNews::Applied(list, mut warnings) => {
                warnings.extend(key_clashes(&list, active(cx)));
                let keys = keys_of(&list);
                state.list = list;
                state.notice = (!warnings.is_empty()).then(|| notice(warnings.join(" · "), false));
                rebind_user_keys(keys, cx);
                self.refresh_user_command_source(window, cx);
            }
            UserCommandNews::Invalid(message) => state.notice = Some(notice(message, true)),
            UserCommandNews::Exited { name, code, hidden } => {
                state.run_notice = code
                    .filter(|code| hidden && *code != 0)
                    .map(|code| notice(t!("user-command-exited", name = name, code = code), true));
            }
            UserCommandNews::Refused { message, .. } => {
                state.run_notice = Some(notice(message, true));
            }
        }
        cx.notify();
    }

    /// Any command but a scroll drops the clipboard, update and user run notices.
    pub(super) fn forget_command_notices(&mut self) {
        self.forget_clipboard_notice();
        self.forget_update_notice();
        self.user_commands.run_notice = None;
    }

    pub(super) fn user_command_notice(&self) -> Option<StatusNotice> {
        let state = &self.user_commands;
        state.run_notice.clone().or_else(|| state.notice.clone())
    }

    #[cfg(test)]
    pub(super) fn user_command_list(&self) -> &[UserCommandInfo] {
        &self.user_commands.list
    }

    /// The listed commands as name and key text, for Ctrl+K.
    pub(super) fn user_command_rows(&self) -> Vec<(String, Option<String>)> {
        let list = &self.user_commands.list;
        list.iter()
            .map(|info| (info.name.clone(), info.key.clone()))
            .collect()
    }

    pub(super) fn run_user_command(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(invocation) = self.user_invocation() else {
            let text = t!("user-command-on-parent", name = name);
            self.user_commands.run_notice = Some(notice(text, true));
            cx.notify();
            return;
        };
        if let Some(run) = &self.user_commands.run {
            run(name, invocation);
        }
    }

    /// The values of the active pane, or `None` when the cursor is on `..`.
    fn user_invocation(&self) -> Option<UserInvocation> {
        let state = self.browser.state();
        let pane = state.focused_pane();
        let tab = state.pane(pane)?.active();
        if cursor_name(tab).is_some_and(|name| name == "..") {
            return None;
        }
        let marked = if tab.selection().marked().is_empty() {
            Vec::new()
        } else {
            self.menu_targets(pane)
                .map(|found| found.items)
                .unwrap_or_default()
        };
        let other = self
            .browser
            .target_pane()
            .and_then(|other| state.pane(other))
            .map(|other| other.active());
        Some(UserInvocation {
            file: cursor_location(tab),
            marked,
            folder: Some(tab.location().clone()),
            other_folder: other.map(|tab| tab.location().clone()),
            other_file: other.and_then(cursor_location),
        })
    }

    fn edit_user_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(file), Some(edit)) = (
            self.user_commands.file.clone(),
            self.user_commands.edit.clone(),
        ) else {
            self.user_commands.run_notice = Some(notice(t!("user-command-no-file"), true));
            cx.notify();
            return;
        };
        self.run_off_thread(
            Box::new(move || {
                let written = edit();
                Box::new(move |view, window, cx| {
                    view.user_commands_written((file, written), window, cx)
                })
            }),
            window,
            cx,
        );
    }

    fn user_commands_written(
        &mut self,
        (file, written): (Location, Result<(), String>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match written {
            Ok(()) => self.open_for_edit((file, false), window, cx),
            Err(message) => self.user_commands.run_notice = Some(notice(message, true)),
        }
    }
}

/// Binds the keys of the run and edit actions on the window root.
pub(super) fn user_command_keys(root: Div, cx: &mut Context<NavoWindow>) -> Div {
    root.on_action(
        cx.listener(|this: &mut NavoWindow, action: &RunUserCommand, _, cx| {
            this.run_user_command(&action.name, cx);
        }),
    )
    .on_action(cx.listener(
        |this: &mut NavoWindow, _: &super::keys::EditUserCommands, window, cx| {
            this.edit_user_commands(window, cx);
        },
    ))
}

/// One warning per user key that a FileList, pane or global command already uses.
fn key_clashes(list: &[UserCommandInfo], keymap: &Keymap) -> Vec<String> {
    let mut found = Vec::new();
    for info in list {
        let Some(key) = info.key.as_deref().and_then(normal_key) else {
            continue;
        };
        let taker = keymap.binds().iter().find(|bind| {
            screen_reach("FileList", &bind.context).is_some_and(|reach| reach != Reach::Inner)
                && bind
                    .keys
                    .iter()
                    .any(|bound| normal_key(bound).as_deref() == Some(key.as_str()))
        });
        if let Some(bind) = taker {
            found.push(t!(
                "user-command-key-clash",
                key = shown(&key).unwrap_or(key),
                name = info.name.clone(),
                command = navo_l10n::tr(bind.spec.label)
            ));
        }
    }
    found
}

fn keys_of(list: &[UserCommandInfo]) -> Vec<(String, String)> {
    list.iter()
        .filter_map(|info| Some((info.key.clone()?, info.name.clone())))
        .collect()
}

/// Rebinds the app keys only when the user command keys changed.
fn rebind_user_keys(keys: Vec<(String, String)>, cx: &mut App) {
    if user_keys(cx) == keys {
        return;
    }
    set_user_keys(cx, keys);
    rebind_app_keys(active(cx).clone(), cx);
}

fn cursor_name(tab: &Tab) -> Option<String> {
    let entry = tab.listing()?.get(tab.view().cursor)?;
    Some(entry.name.clone())
}

fn cursor_location(tab: &Tab) -> Option<Location> {
    tab.entry_location(&cursor_name(tab)?).ok()
}

fn notice(text: String, is_error: bool) -> StatusNotice {
    StatusNotice {
        text: text.into(),
        is_error,
    }
}
