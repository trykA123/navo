use std::rc::Rc;

use gpui_kit::{App, Div, FocusHandle, InteractiveElement, KeyBinding, Stateful, Window};

use super::Handler;

pub(super) const CONTEXT: &str = "ConfirmDialog";

gpui_kit::actions!(
    confirm_dialog,
    [Dismiss, Activate, FocusNext, FocusPrevious]
);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, context),
        KeyBinding::new("enter", Activate, context),
        KeyBinding::new("tab", FocusNext, context),
        KeyBinding::new("shift-tab", FocusPrevious, context),
    ]);
}

pub(super) struct Keys {
    pub stops: Vec<FocusHandle>,
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
        if self.cancel.is_focused(window) {
            self.cancel(window, cx);
        } else if self.can_confirm && self.confirm.is_focused(window) {
            self.confirm(window, cx);
        }
    }

    fn cycle(&self, forward: bool, window: &mut Window, cx: &mut App) {
        let count = self.stops.len();
        let current = self.stops.iter().position(|stop| stop.is_focused(window));
        let next = match (current, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, _) => 0,
        };
        if let Some(stop) = self.stops.get(next) {
            window.focus(stop, cx);
        }
    }
}

pub(super) fn register(root: Stateful<Div>, keys: &Rc<Keys>) -> Stateful<Div> {
    let dismiss = keys.clone();
    let activate = keys.clone();
    let next = keys.clone();
    let previous = keys.clone();
    root.key_context(CONTEXT)
        .on_action(move |_: &Dismiss, window, cx| dismiss.cancel(window, cx))
        .on_action(move |_: &Activate, window, cx| activate.activate(window, cx))
        .on_action(move |_: &FocusNext, window, cx| next.cycle(true, window, cx))
        .on_action(move |_: &FocusPrevious, window, cx| previous.cycle(false, window, cx))
}
