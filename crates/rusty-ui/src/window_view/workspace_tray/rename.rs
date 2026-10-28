use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, Focusable, KeyDownEvent, Subscription};
use rusty_core::{Command, MAX_WORKSPACES};

use super::super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::{Kbd, TextField, TextFieldState, text_field_state};
use crate::status_bar::StatusNotice;
use crate::tokens::Theme;

pub(in crate::window_view) struct TrayEdit {
    pub(in crate::window_view) cards: [FocusHandle; MAX_WORKSPACES],
    pub(super) rename: Option<Rename>,
    pub(super) deleted: Option<Deleted>,
}

pub(super) struct Rename {
    pub(super) n: usize,
    input: Entity<TextFieldState>,
    _enter: Subscription,
}

pub(super) struct Deleted {
    pub(super) after_job: Option<u64>,
}

impl TrayEdit {
    pub(in crate::window_view) fn new(cx: &mut App) -> Self {
        Self {
            cards: std::array::from_fn(|_| cx.focus_handle()),
            rename: None,
            deleted: None,
        }
    }

    pub(in crate::window_view) fn has_focus(&self, window: &Window, cx: &App) -> bool {
        let is_renaming = self
            .rename
            .as_ref()
            .is_some_and(|rename| rename.input.read(cx).focus_handle(cx).is_focused(window));
        is_renaming
            || self
                .cards
                .iter()
                .any(|card| card.contains_focused(window, cx))
    }

    pub(in crate::window_view) fn drop_rename(&mut self) {
        self.rename = None;
    }
}

impl FerriteWindow {
    pub(super) fn workspace_name(&self, n: usize) -> Option<String> {
        let state = self.browser.state();
        let core = state.window(state.focused_window())?;
        Some(core.workspaces().get(n.checked_sub(1)?)?.name.clone())
    }

    pub(super) fn start_workspace_rename(
        &mut self,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.workspace_name(n) else {
            return;
        };
        let input = text_field_state(rusty_l10n::tr("workspace-name"), window, cx);
        input.update(cx, |field, cx| {
            field.set_value(name, window, cx);
            field.select_all(window, cx);
        });
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.keep_workspace_rename(window, cx);
            }
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.workspace_keys.edit.rename = Some(Rename {
            n,
            input,
            _enter: enter,
        });
        cx.notify();
    }

    fn keep_workspace_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = &self.workspace_keys.edit.rename else {
            return;
        };
        let command = Command::WorkspaceRename {
            n: rename.n,
            name: rename.input.read(cx).value().to_string(),
        };
        if self.browser.command(&command).is_err() {
            self.show_notice(StatusNotice {
                text: rusty_l10n::tr("workspace-name-invalid").into(),
                is_error: true,
            });
            cx.notify();
            return;
        }
        self.end_workspace_rename(window, cx);
    }

    pub(in crate::window_view) fn end_workspace_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_keys.edit.rename.take().is_some() {
            self.focus_core_pane(window, cx);
        }
        cx.notify();
    }

    pub(super) fn workspace_rename_row(
        &self,
        t: &Theme,
        n: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let rename = self
            .workspace_keys
            .edit
            .rename
            .as_ref()
            .filter(|rename| rename.n == n)?;
        Some(
            div()
                .id(("workspace-rename-field", n))
                .test_support()
                .flex()
                .items_center()
                .gap(t.spacing.sm)
                .w_full()
                .children(Kbd::new(("workspace-key", n), &n.to_string()))
                .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        view.end_workspace_rename(window, cx);
                    }
                }))
                .child(div().flex_1().min_w_0().child(TextField::new(
                    "workspace-name",
                    &rename.input,
                    rusty_l10n::tr("workspace-name"),
                )))
                .into_any_element(),
        )
    }
}
