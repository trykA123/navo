use gpui_kit::{AnyWindowHandle, KeystrokeEvent, Modifiers, Subscription, Task, WeakEntity};
use rusty_core::Command;

use super::FerriteWindow;
use super::workspace_tray::TrayEdit;
use crate::file_list::FilterEdit;
use crate::fkey_bar::KeyLayer;
use crate::modifier_hints::{Arm, Combo, Delay, Hold};
use crate::prelude::*;
use crate::status_bar::{WorkspaceButton, WorkspaceSummary};

gpui_kit::actions!(
    ferrite_window,
    [
        PreviewWorkspace1,
        PreviewWorkspace2,
        PreviewWorkspace3,
        PreviewWorkspace4,
        PreviewWorkspace5,
        PreviewWorkspace6,
        PreviewWorkspace7,
        PreviewWorkspace8,
        PreviewWorkspace9,
        CancelWorkspacePreview,
        NewWorkspace,
        SaveWorkspace,
        MoveWorkspaceLeft,
        MoveWorkspaceRight
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Preview {
    Existing(usize),
    New,
}

pub(super) struct WorkspaceKeys {
    pub(super) visible: bool,
    pub(super) preview: Option<Preview>,
    pub(super) edit: TrayEdit,
    pub(super) pinned: bool,
    alt: bool,
    hold: Hold,
    timer: Option<Task<()>>,
    _keys: Subscription,
}

fn intercept_key(
    (this, own): (&WeakEntity<FerriteWindow>, AnyWindowHandle),
    event: &KeystrokeEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let key = event.keystroke.key.as_str();
    let is_modifier = matches!(
        key,
        "control" | "shift" | "alt" | "platform" | "function" | "capslock"
    );
    if window.window_handle() != own || is_modifier {
        return;
    }
    let is_unfocused_escape = key == "escape" && window.focused(cx).is_none();
    let _ = this.update(cx, |view, cx| {
        view.note_key(is_unfocused_escape, window, cx);
    });
}

impl WorkspaceKeys {
    pub(super) fn new(window: &Window, cx: &mut Context<FerriteWindow>) -> Self {
        let this = cx.weak_entity();
        let own: AnyWindowHandle = window.window_handle();
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            intercept_key((&this, own), event, window, cx);
        });
        Self {
            visible: false,
            preview: None,
            edit: TrayEdit::new(cx),
            pinned: false,
            alt: false,
            hold: Hold::default(),
            timer: None,
            _keys: keys,
        }
    }

    pub(super) fn cancel(&mut self) {
        self.preview = None;
        self.visible = false;
        self.pinned = false;
        self.hold.cancel();
        self.timer = None;
    }

    fn pin(&mut self) {
        self.pinned = true;
        self.visible = true;
    }

    fn release(&mut self) -> Option<Preview> {
        let (preview, is_pinned) = (self.preview, self.pinned);
        self.cancel();
        if is_pinned {
            self.pin();
        }
        preview
    }
}

impl FerriteWindow {
    pub(super) fn track_modifiers(
        &mut self,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = KeyLayer::held(&modifiers).filter(|_| window.is_window_active());
        if held != self.hints.held {
            self.hints.held = held;
            cx.notify();
        }
        self.workspace_modifiers(modifiers, window, cx);
        self.hint_modifiers(modifiers, window, cx);
        self.drag_modifiers(&modifiers, cx);
    }

    pub(super) fn sync_workspaces(&mut self, window: &Window, cx: &App) {
        if !self.workspaces_allowed(window) && !self.tray_kept(window, cx) {
            self.workspace_keys.cancel();
            self.workspace_keys.alt = false;
            self.workspace_keys.edit.drop_rename();
        }
    }

    pub(super) fn workspaces_allowed(&self, window: &Window) -> bool {
        window.is_window_active() && self.pane_has_focus(window)
    }

    pub(super) fn pane_has_focus(&self, window: &Window) -> bool {
        let pane = self.browser.state().focused_pane();
        self.viewer.is_none()
            && self
                .focus
                .get(&pane)
                .is_some_and(|focus| focus.is_focused(window))
    }

    fn tray_kept(&self, window: &Window, cx: &App) -> bool {
        self.workspace_keys.edit.has_focus(window, cx)
            || (self.workspace_keys.pinned && self.pane_has_focus(window))
    }

    pub(super) fn toggle_workspace_pin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_keys.pinned {
            self.workspace_keys.cancel();
            self.end_workspace_rename(window, cx);
        } else {
            self.pin_workspaces(window, cx);
        }
    }

    fn overlay_owns_focus(&self) -> bool {
        self.settings.is_open()
            || self.recent.is_open()
            || self.command_center.is_open()
            || self.viewer.is_some()
            || self.prompt.is_some()
            || self.is_multi_rename_open()
    }

    pub(super) fn pin_workspaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay_owns_focus() {
            return;
        }
        self.workspace_keys.pin();
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    fn note_key(&mut self, is_unfocused_escape: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace_keys.hold.key_pressed();
        if is_unfocused_escape {
            self.escape_unfocused(window, cx);
        }
    }

    fn escape_unfocused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay_owns_focus() || !self.preview.is_open() {
            return;
        }
        self.focus_core_pane(window, cx);
        let tab = self.browser.state().active_tab().id();
        if self.browser.filter_text(tab).is_some() {
            self.filter(FilterEdit::Clear);
        } else if !self.stop_find() {
            self.escape_pane(cx);
        }
        cx.notify();
    }

    pub(super) fn escape_pane(&mut self, cx: &mut Context<Self>) {
        if self.workspace_keys.pinned {
            self.workspace_keys.cancel();
            cx.notify();
        } else {
            self.escape_preview(cx);
        }
    }

    pub(super) fn workspace_button(&self, cx: &mut Context<Self>) -> Option<WorkspaceButton> {
        let state = self.browser.state();
        let core = state.window(state.focused_window())?;
        let active = core.active_workspace();
        let workspace = core.workspaces().get(active)?;
        let summary = WorkspaceSummary {
            name: workspace.name.clone().into(),
            n: active + 1,
            total: core.workspaces().len(),
        };
        Some(
            WorkspaceButton::new(summary)
                .pinned(self.workspace_keys.pinned)
                .on_click(cx.listener(|view, _, window, cx| {
                    view.toggle_workspace_pin(window, cx);
                })),
        )
    }

    pub(super) fn workspace_modifiers(
        &mut self,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let released = self.workspace_keys.alt && !modifiers.alt;
        self.workspace_keys.alt = modifiers.alt;
        if !self.workspaces_allowed(window) {
            if self.tray_kept(window, cx) {
                self.workspace_keys.preview = None;
            } else {
                self.workspace_keys.cancel();
                self.workspace_keys.alt = false;
            }
            return;
        }
        if released {
            match self.workspace_keys.release() {
                Some(Preview::Existing(n)) => {
                    self.apply(&Command::WorkspaceSwitch { n }, window, cx);
                }
                Some(Preview::New) => self.create_workspace(window, cx),
                None => {}
            }
            cx.notify();
        }
        let held = Combo::of(&modifiers).filter(|combo| *combo == Combo::ALT);
        let delay = Delay::from_ms(self.values.modifier_hints_delay_ms());
        let arm = self.workspace_keys.hold.modifiers(held, delay);
        if let Some(arm) = arm {
            self.arm_workspace(arm, window, cx);
        }
    }

    fn arm_workspace(&mut self, arm: Arm, window: &Window, cx: &mut Context<Self>) {
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(arm.delay).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.workspace_due(arm.token, window, cx);
            });
        });
        self.workspace_keys.timer = Some(task);
    }

    fn workspace_due(&mut self, token: u64, window: &Window, cx: &mut Context<Self>) {
        if self.workspaces_allowed(window) && self.workspace_keys.hold.elapsed(token) {
            self.workspace_keys.visible = true;
            cx.notify();
        }
    }

    fn preview_workspace(&mut self, n: usize, window: &Window, cx: &mut Context<Self>) {
        if !self.workspaces_allowed(window) {
            return;
        }
        let state = self.browser.state();
        let Some(core) = state.window(state.focused_window()) else {
            return;
        };
        self.workspace_keys.preview = if n == core.active_workspace() + 1 {
            None
        } else if n <= core.workspaces().len() {
            Some(Preview::Existing(n))
        } else {
            Some(Preview::New)
        };
        self.workspace_keys.visible = true;
        self.workspace_keys.alt = window.modifiers().alt;
        cx.notify();
    }

    fn workspace_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if !self.workspaces_allowed(window) {
            return;
        }
        self.workspace_keys.cancel();
        self.apply(&command, window, cx);
    }

    fn move_workspace(&mut self, by: i8, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.browser.state();
        let Some(core) = state.window(state.focused_window()) else {
            return;
        };
        let n = core.active_workspace() + 1;
        self.workspace_command(Command::WorkspaceMove { n, by }, window, cx);
    }
}

macro_rules! preview_keys {
    ($root:expr, $cx:expr, $($action:ident => $n:literal),+ $(,)?) => {
        $root$(.on_action($cx.listener(|view: &mut FerriteWindow, _: &$action, window, cx| {
            view.preview_workspace($n, window, cx);
        })))+
    };
}

pub(super) fn workspace_keys(root: Div, cx: &mut Context<FerriteWindow>) -> Div {
    let root = super::layout_mode::layout_keys(root, cx);
    let root = preview_keys!(root, cx,
        PreviewWorkspace1 => 1,
        PreviewWorkspace2 => 2,
        PreviewWorkspace3 => 3,
        PreviewWorkspace4 => 4,
        PreviewWorkspace5 => 5,
        PreviewWorkspace6 => 6,
        PreviewWorkspace7 => 7,
        PreviewWorkspace8 => 8,
        PreviewWorkspace9 => 9,
    );
    root.on_action(cx.listener(|view, _: &CancelWorkspacePreview, _, cx| {
        view.workspace_keys.cancel();
        cx.notify();
    }))
    .on_action(cx.listener(|view, _: &NewWorkspace, window, cx| {
        if view.workspaces_allowed(window) {
            view.workspace_keys.cancel();
            view.create_workspace(window, cx);
        }
    }))
    .on_action(cx.listener(|view, _: &SaveWorkspace, window, cx| {
        view.workspace_command(Command::WorkspaceSave, window, cx);
    }))
    .on_action(cx.listener(|view, _: &MoveWorkspaceLeft, window, cx| {
        view.move_workspace(-1, window, cx);
    }))
    .on_action(cx.listener(|view, _: &MoveWorkspaceRight, window, cx| {
        view.move_workspace(1, window, cx);
    }))
}
