use std::rc::Rc;

use gpui_kit::{App, Div, Entity, FocusHandle, InteractiveElement, Stateful, Window};

use super::{Cancel, Submit};
use crate::dialog_focus::{self, Next, NextButton, Press, Previous, PreviousButton};
use crate::primitives::TextFieldState;

const KEY_CONTEXT: &str = "PromptDialog Dialog";
pub(super) const BUTTON: &str = "PromptButton DialogButton";

gpui_kit::actions!(prompt_dialog, [Dismiss]);

pub fn bind_keys(cx: &mut App) {
    dialog_focus::bind_keys(cx);
    crate::commands::bind_namespace(cx, "prompt_dialog");
}

pub(super) struct Keys {
    pub field: Entity<TextFieldState>,
    pub field_focus: FocusHandle,
    pub confirm: FocusHandle,
    pub cancel: FocusHandle,
    pub on_submit: Option<Submit>,
    pub on_cancel: Option<Cancel>,
}

impl Keys {
    pub(super) fn submit(&self, window: &mut Window, cx: &mut App) {
        let value = self.field.read(cx).value();
        if let Some(handler) = &self.on_submit {
            handler(&value, window, cx);
        }
    }

    pub(super) fn cancel(&self, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_cancel {
            handler(window, cx);
        }
    }

    fn press(&self, window: &mut Window, cx: &mut App) {
        if self.confirm.contains_focused(window, cx) {
            self.submit(window, cx);
        } else if self.cancel.contains_focused(window, cx) {
            self.cancel(window, cx);
        }
    }

    fn stops(&self) -> [FocusHandle; 3] {
        [
            self.field_focus.clone(),
            self.confirm.clone(),
            self.cancel.clone(),
        ]
    }

    fn buttons(&self) -> [FocusHandle; 2] {
        [self.confirm.clone(), self.cancel.clone()]
    }
}

pub(super) fn register(root: Stateful<Div>, keys: &Rc<Keys>) -> Stateful<Div> {
    let (dismiss, press) = (keys.clone(), keys.clone());
    let (next, previous) = (keys.clone(), keys.clone());
    let (next_button, previous_button) = (keys.clone(), keys.clone());
    root.key_context(KEY_CONTEXT)
        .on_action(move |_: &Dismiss, window, cx| dismiss.cancel(window, cx))
        .on_action(move |_: &Press, window, cx| press.press(window, cx))
        .on_action(move |_: &Next, window, cx| {
            dialog_focus::cycle(&next.stops(), true, window, cx);
        })
        .on_action(move |_: &Previous, window, cx| {
            dialog_focus::cycle(&previous.stops(), false, window, cx);
        })
        .on_action(move |_: &NextButton, window, cx| {
            dialog_focus::cycle(&next_button.buttons(), true, window, cx);
        })
        .on_action(move |_: &PreviousButton, window, cx| {
            dialog_focus::cycle(&previous_button.buttons(), false, window, cx);
        })
}
