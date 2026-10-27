use gpui_kit::{AnyWindowHandle, Modifiers, Subscription, Task};
use rusty_core::Command;

use super::FerriteWindow;
use crate::fkey_bar::KeyLayer;
use crate::modifier_hints::{Arm, Combo, Delay, Hold};
use crate::prelude::*;

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
    alt: bool,
    hold: Hold,
    timer: Option<Task<()>>,
    _keys: Subscription,
}

impl WorkspaceKeys {
    pub(super) fn new(window: &Window, cx: &mut Context<FerriteWindow>) -> Self {
        let this = cx.weak_entity();
        let own: AnyWindowHandle = window.window_handle();
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            let key = event.keystroke.key.as_str();
            if window.window_handle() == own
                && !matches!(
                    key,
                    "control" | "shift" | "alt" | "platform" | "function" | "capslock"
                )
            {
                let _ = this.update(cx, |view, _| view.workspace_keys.hold.key_pressed());
            }
        });
        Self {
            visible: false,
            preview: None,
            alt: false,
            hold: Hold::default(),
            timer: None,
            _keys: keys,
        }
    }

    pub(super) fn cancel(&mut self) {
        self.preview = None;
        self.visible = false;
        self.hold.cancel();
        self.timer = None;
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

    pub(super) fn sync_workspaces(&mut self, window: &Window) {
        if !self.workspaces_allowed(window) {
            self.workspace_keys.cancel();
            self.workspace_keys.alt = false;
        }
    }

    pub(super) fn workspaces_allowed(&self, window: &Window) -> bool {
        let pane = self.browser.state().focused_pane();
        window.is_window_active()
            && self.viewer.is_none()
            && self
                .focus
                .get(&pane)
                .is_some_and(|focus| focus.is_focused(window))
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
            self.workspace_keys.cancel();
            self.workspace_keys.alt = false;
            return;
        }
        if released {
            let preview = self.workspace_keys.preview;
            self.workspace_keys.cancel();
            if let Some(preview) = preview {
                let command = match preview {
                    Preview::Existing(n) => Command::WorkspaceSwitch { n },
                    Preview::New => Command::WorkspaceNew { name: None },
                };
                self.apply(&command, window, cx);
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
        view.workspace_command(Command::WorkspaceNew { name: None }, window, cx);
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
