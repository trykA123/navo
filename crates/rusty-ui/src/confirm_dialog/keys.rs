use std::rc::Rc;

use gpui_kit::{App, Div, FocusHandle, InteractiveElement, Stateful, Window};

use super::Handler;
use crate::dialog_focus::{self, Next, NextButton, Press, Previous, PreviousButton};

const KEY_CONTEXT: &str = "ConfirmDialog Dialog";

pub(super) const BUTTON: &str = "ConfirmButton DialogButton";

gpui_kit::actions!(confirm_dialog, [Dismiss]);

pub fn bind_keys(cx: &mut App) {
    dialog_focus::bind_keys(cx);
    crate::commands::bind_namespace(cx, "confirm_dialog");
}

pub(super) struct Keys {
    pub stops: Vec<FocusHandle>,
    pub buttons: Vec<FocusHandle>,
    pub confirm: FocusHandle,
    pub can_confirm: bool,
    pub cancel: FocusHandle,
    pub on_confirm: Option<Handler>,
    pub on_cancel: Option<Handler>,
}

impl Keys {
    pub(super) fn cancel(&self, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_cancel {
            handler(window, cx);
        }
    }

    pub(super) fn confirm(&self, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_confirm {
            handler(window, cx);
        }
    }

    fn activate(&self, window: &mut Window, cx: &mut App) {
        if self.cancel.contains_focused(window, cx) {
            self.cancel(window, cx);
        } else if self.can_confirm && self.confirm.contains_focused(window, cx) {
            self.confirm(window, cx);
        }
    }
}

pub(super) fn register(root: Stateful<Div>, keys: &Rc<Keys>) -> Stateful<Div> {
    let dismiss = keys.clone();
    let activate = keys.clone();
    let next = keys.clone();
    let previous = keys.clone();
    let next_button = keys.clone();
    let previous_button = keys.clone();
    root.key_context(KEY_CONTEXT)
        .on_action(move |_: &Dismiss, window, cx| dismiss.cancel(window, cx))
        .on_action(move |_: &Press, window, cx| activate.activate(window, cx))
        .on_action(move |_: &Next, window, cx| dialog_focus::cycle(&next.stops, true, window, cx))
        .on_action(move |_: &Previous, window, cx| {
            dialog_focus::cycle(&previous.stops, false, window, cx);
        })
        .on_action(move |_: &NextButton, window, cx| {
            dialog_focus::cycle(&next_button.buttons, true, window, cx);
        })
        .on_action(move |_: &PreviousButton, window, cx| {
            dialog_focus::cycle(&previous_button.buttons, false, window, cx);
        })
}
