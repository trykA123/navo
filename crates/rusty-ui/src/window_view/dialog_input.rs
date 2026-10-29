use std::collections::HashSet;

use gpui_kit::{KeyDownEvent, KeyUpEvent, KeystrokeEvent, Subscription};

use super::FerriteWindow;
use crate::prelude::*;

#[derive(Default)]
pub(super) struct DialogInput {
    owned: HashSet<String>,
    waiting: bool,
    in_dialog: bool,
    pub(super) blocked: bool,
}

impl FerriteWindow {
    pub(super) fn watch_dialog_input(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let this = cx.weak_entity();
        let own = window.window_handle();
        cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != own {
                return;
            }
            let _ = this.update(cx, |view, _| view.note_dialog_key(event));
        })
    }

    fn note_dialog_key(&mut self, event: &KeystrokeEvent) {
        self.dialog_input.in_dialog = self.dialog_is_open()
            || event
                .context_stack
                .iter()
                .any(|context| context.contains("Dialog"));
        if self.dialog_input.in_dialog {
            self.dialog_input.owned.insert(event.keystroke.key.clone());
            self.dialog_input.waiting = true;
            self.dialog_input.blocked = true;
        }
    }

    pub(super) fn dialog_is_open(&self) -> bool {
        self.prompt.is_some() || self.browser.pending().is_some()
    }

    pub(super) fn dialog_input_on(root: Div, cx: &mut Context<Self>) -> Div {
        root.capture_key_down(cx.listener(Self::dialog_key_down))
            .capture_key_up(cx.listener(|view, event: &KeyUpEvent, _, _| {
                view.dialog_input.owned.remove(&event.keystroke.key);
            }))
    }

    fn dialog_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, _: &mut Context<Self>) {
        if self.dialog_input.in_dialog {
            return;
        }
        let input = &mut self.dialog_input;
        if !event.is_held {
            input.waiting = false;
            input.owned.remove(&event.keystroke.key);
        }
        input.blocked = input.waiting || input.owned.contains(&event.keystroke.key);
    }
}
