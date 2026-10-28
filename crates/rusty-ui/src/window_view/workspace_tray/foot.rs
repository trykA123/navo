use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_core::Command;
use rusty_l10n::{t, tr};

use super::super::FerriteWindow;
use super::super::strip::undo_key;
use super::card::{OnTrayAction, TrayAction};
use super::rename::Deleted;
use crate::commands::active;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName, Kbd};
use crate::status_bar::StatusNotice;
use crate::tokens::Theme;

pub(super) struct Foot<'a> {
    pub(super) n: usize,
    pub(super) name: &'a str,
    pub(super) count: usize,
    pub(super) draft: u16,
    pub(super) active: bool,
}

pub(super) fn run(
    act: &OnTrayAction,
    action: TrayAction,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let act = act.clone();
    move |_, window, cx| act(&action, window, cx)
}

fn draft_foot(t: &Theme, foot: &Foot, act: &OnTrayAction) -> Div {
    let n = foot.n;
    let label = t!("workspace-draft", count = foot.draft);
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .w_full()
        .min_w_0()
        .child(
            div()
                .id(("workspace-draft", n))
                .role(Role::Status)
                .aria_label(label.clone())
                .test_support()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.warn)
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(t.spacing.xs)
                .min_w_0()
                .child(
                    Button::new(("workspace-save", n), tr("workspace-save"))
                        .variant(ButtonVariant::Primary)
                        .disabled(!foot.active)
                        .on_click(run(act, TrayAction::Save)),
                )
                .child(
                    Button::new(("workspace-discard", n), tr("workspace-discard"))
                        .on_click(run(act, TrayAction::Discard(n))),
                ),
        )
}

fn tools_foot(t: &Theme, foot: &Foot, act: &OnTrayAction) -> Div {
    let n = foot.n;
    let mover = |id: &'static str, icon, by: i8, is_disabled: bool| {
        IconButton::new((id, n), icon, tr(id))
            .disabled(is_disabled)
            .on_click(run(act, TrayAction::Move { n, by }))
    };
    div()
        .flex()
        .items_center()
        .justify_end()
        .gap(t.spacing.xxs)
        .min_w_0()
        .child(
            IconButton::new(
                ("workspace-rename", n),
                IconName::Pencil,
                t!("workspace-rename", name = foot.name.to_owned()),
            )
            .on_click(run(act, TrayAction::Rename(n))),
        )
        .child(mover(
            "workspace-move-left",
            IconName::ChevronLeft,
            -1,
            n == 1,
        ))
        .child(mover(
            "workspace-move-right",
            IconName::ChevronRight,
            1,
            n == foot.count,
        ))
        .child(
            IconButton::new(
                ("workspace-delete", n),
                IconName::Trash,
                t!("workspace-delete", name = foot.name.to_owned()),
            )
            .disabled(foot.count <= 1)
            .on_click(run(act, TrayAction::Delete(n))),
        )
}

pub(super) fn card_foot(t: &Theme, foot: &Foot, act: &OnTrayAction) -> Div {
    let row = if foot.draft > 0 {
        draft_foot(t, foot, act)
    } else {
        tools_foot(t, foot, act)
    };
    row.px(t.spacing.xs)
}

impl FerriteWindow {
    fn workspace_count(&self) -> usize {
        let state = self.browser.state();
        state
            .window(state.focused_window())
            .map_or(0, |core| core.workspaces().len())
    }

    fn last_undo_job(&self) -> Option<u64> {
        let last = self.browser.rename_hooks()?.last.clone();
        last().ok().flatten().map(|offer| offer.job)
    }

    pub(in crate::window_view) fn delete_workspace(
        &mut self,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.workspace_count();
        let Some(name) = self.workspace_name(n).filter(|_| count > 1) else {
            return;
        };
        let after_job = self.last_undo_job();
        self.apply(&Command::WorkspaceDelete { n }, window, cx);
        if self.workspace_count() == count {
            return;
        }
        self.workspace_keys.edit.deleted = Some(Deleted { after_job });
        let text = match undo_key(cx) {
            Some(key) => t!("workspace-deleted-key", name = name, key = key.to_string()),
            None => t!("workspace-deleted", name = name),
        };
        self.show_notice(StatusNotice {
            text: text.into(),
            is_error: false,
        });
        cx.notify();
    }

    pub(in crate::window_view) fn create_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.workspace_count();
        self.apply(&Command::WorkspaceNew { name: None }, window, cx);
        let n = self.workspace_count();
        let Some(name) = self.workspace_name(n).filter(|_| n > count) else {
            return;
        };
        let text = match preview_key(n, cx) {
            Some(key) => t!("workspace-created-key", name = name, key = key.to_string()),
            None => t!("workspace-created", name = name),
        };
        self.show_notice(StatusNotice {
            text: text.into(),
            is_error: false,
        });
        cx.notify();
    }

    pub(in crate::window_view) fn restores_workspace(&self) -> bool {
        self.workspace_keys
            .edit
            .deleted
            .as_ref()
            .is_some_and(|deleted| deleted.after_job == self.last_undo_job())
    }

    pub(in crate::window_view) fn restore_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_keys.edit.deleted = None;
        self.apply(&Command::WorkspaceRestore, window, cx);
    }
}

fn preview_key(n: usize, cx: &App) -> Option<SharedString> {
    const ORDINALS: [&str; 9] = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
    ];
    let command = format!("workspace.preview_{}", ORDINALS.get(n.checked_sub(1)?)?);
    let key = active(cx).keys_of(&command).next()?;
    Kbd::new("workspace-created-key", key).map(|kbd| kbd.text().into())
}
