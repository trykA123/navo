use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, Role};
use rusty_core::{Command, Snapshot};

mod card;
mod foot;
mod rename;

pub use card::{OnTrayAction, TrayAction, TrayCard, TrayTile, WorkspaceTray};
pub(super) use rename::TrayEdit;

use super::FerriteWindow;
use super::workspace_keys::Preview;
use crate::prelude::*;
use crate::tokens::{self, Theme};

fn tiles(snapshot: &Snapshot) -> Vec<TrayTile> {
    let panes = snapshot.panes();
    snapshot
        .layout()
        .rects()
        .into_iter()
        .filter_map(|(id, rect)| {
            let pane = panes.iter().find(|pane| pane.id() == id)?;
            let folder = pane.active().location();
            let label = folder.name().map_or_else(
                || folder.path().display().into_owned(),
                |name| name.display().into_owned(),
            );
            Some(TrayTile {
                x: rect.x as f32,
                y: rect.y as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                label: label.into(),
            })
        })
        .collect()
}

impl FerriteWindow {
    fn choose_workspace(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace_keys.cancel();
        self.apply(&command, window, cx);
    }

    fn tray_action(&mut self, action: TrayAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            TrayAction::Switch(n) => {
                self.choose_workspace(Command::WorkspaceSwitch { n }, window, cx);
            }
            TrayAction::New => {
                self.workspace_keys.cancel();
                self.create_workspace(window, cx);
            }
            TrayAction::Close => {
                self.workspace_keys.cancel();
                self.end_workspace_rename(window, cx);
            }
            TrayAction::Rename(n) => self.start_workspace_rename(n, window, cx),
            TrayAction::Move { n, by } => {
                self.apply(&Command::WorkspaceMove { n, by }, window, cx);
            }
            TrayAction::Delete(n) => self.delete_workspace(n, window, cx),
            TrayAction::Save => self.apply(&Command::WorkspaceSave, window, cx),
            TrayAction::Discard(n) => self.apply(&Command::WorkspaceDiscard { n }, window, cx),
        }
    }

    fn tray_cards(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<TrayCard> {
        let state = self.browser.state();
        let Some(core) = state.window(state.focused_window()) else {
            return Vec::new();
        };
        let current = core.snapshot();
        core.workspaces()
            .iter()
            .enumerate()
            .map(|(index, workspace)| {
                let n = index + 1;
                let active = index == core.active_workspace();
                let snapshot = if active {
                    &current
                } else {
                    workspace.live.as_ref().unwrap_or(&workspace.saved)
                };
                TrayCard {
                    name: workspace.name.clone().into(),
                    tiles: tiles(snapshot),
                    active,
                    preview: self.workspace_keys.preview == Some(Preview::Existing(n)),
                    focus: self.workspace_keys.edit.cards.get(index).cloned(),
                    rename: self.workspace_rename_row(t, n, cx),
                }
            })
            .collect()
    }

    pub(super) fn workspace_tray(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.workspace_keys.visible {
            return None;
        }
        let t = tokens::current(cx).clone();
        let inset = if self.fkey_strip_height(&t) > px(0.) {
            px(0.)
        } else {
            t.spacing.md
        };
        let on_action = cx.listener(|view, action: &TrayAction, window, cx| {
            view.tray_action(*action, window, cx);
        });
        let tray = WorkspaceTray::new(self.tray_cards(&t, cx), on_action)
            .new_preview(self.workspace_keys.preview == Some(Preview::New));
        let layer = div()
            .id("workspace-tray-layer")
            .absolute()
            .left_0()
            .right_0()
            .bottom(t.spacing.status_height + t.spacing.xs + inset)
            .child(tray);
        Some(if cx.reduce_motion() {
            layer.into_any_element()
        } else {
            layer
                .with_animation(
                    "workspace-tray-enter",
                    Animation::new(t.motion.fade).with_easing(ease_out_cubic),
                    |layer, delta| layer.opacity(delta),
                )
                .into_any_element()
        })
    }

    pub(super) fn workspace_ribbon(&self, cx: &App) -> Option<AnyElement> {
        let preview = self.workspace_keys.preview?;
        let state = self.browser.state();
        let core = state.window(state.focused_window())?;
        let name = match preview {
            Preview::Existing(n) => core.workspaces().get(n.checked_sub(1)?)?.name.clone(),
            Preview::New => rusty_l10n::tr("workspace-new-from-layout"),
        };
        let label = rusty_l10n::t!("workspace-preview-ribbon", name = name);
        let t = tokens::current(cx);
        Some(
            div()
                .id("workspace-preview-ribbon")
                .role(Role::Status)
                .aria_label(label.clone())
                .test_support()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.accent_text)
                .child(label)
                .into_any_element(),
        )
    }
}
