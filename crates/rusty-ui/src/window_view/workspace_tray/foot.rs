use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_core::Command;
use rusty_l10n::{t, tr};

use super::super::FerriteWindow;
use super::super::strip::undo_key;
use super::rename::Deleted;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName};
use crate::status_bar::StatusNotice;
use crate::tokens::Theme;

pub(super) struct Foot<'a> {
    pub(super) n: usize,
    pub(super) name: &'a str,
    pub(super) count: usize,
    pub(super) draft: u16,
    pub(super) active: bool,
}

fn draft_foot(t: &Theme, foot: &Foot, cx: &mut Context<FerriteWindow>) -> Div {
    let n = foot.n;
    let label = t!("workspace-draft", count = foot.draft);
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w_full()
        .child(
            div()
                .id(("workspace-draft", n))
                .role(Role::Status)
                .aria_label(label.clone())
                .test_support()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.warn)
                .child(label),
        )
        .child(
            Button::new(("workspace-save", n), tr("workspace-save"))
                .variant(ButtonVariant::Primary)
                .disabled(!foot.active)
                .on_click(cx.listener(|view, _, window, cx| {
                    view.apply(&Command::WorkspaceSave, window, cx);
                })),
        )
        .child(
            Button::new(("workspace-discard", n), tr("workspace-discard")).on_click(cx.listener(
                move |view, _, window, cx| {
                    view.apply(&Command::WorkspaceDiscard { n }, window, cx);
                },
            )),
        )
}

fn tools_foot(t: &Theme, foot: &Foot, cx: &mut Context<FerriteWindow>) -> Div {
    let n = foot.n;
    let mover = |id: &'static str, icon, label: &str, by: i8, is_disabled: bool| {
        IconButton::new((id, n), icon, tr(label))
            .disabled(is_disabled)
            .on_click(cx.listener(move |view, _, window, cx| {
                view.apply(&Command::WorkspaceMove { n, by }, window, cx);
            }))
    };
    let left = mover(
        "workspace-move-left",
        IconName::ChevronLeft,
        "workspace-move-left",
        -1,
        n == 1,
    );
    let right = mover(
        "workspace-move-right",
        IconName::ChevronRight,
        "workspace-move-right",
        1,
        n == foot.count,
    );
    div()
        .flex()
        .items_center()
        .justify_end()
        .gap(t.spacing.xxs)
        .child(
            IconButton::new(
                ("workspace-rename", n),
                IconName::Pencil,
                t!("workspace-rename", name = foot.name.to_owned()),
            )
            .on_click(cx.listener(move |view, _, window, cx| {
                view.start_workspace_rename(n, window, cx);
            })),
        )
        .child(left)
        .child(right)
        .child(
            IconButton::new(
                ("workspace-delete", n),
                IconName::Trash,
                t!("workspace-delete", name = foot.name.to_owned()),
            )
            .disabled(foot.count <= 1)
            .on_click(cx.listener(move |view, _, window, cx| {
                view.delete_workspace(n, window, cx);
            })),
        )
}

pub(super) fn card_foot(t: &Theme, foot: &Foot, cx: &mut Context<FerriteWindow>) -> Div {
    if foot.draft > 0 {
        draft_foot(t, foot, cx)
    } else {
        tools_foot(t, foot, cx)
    }
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
