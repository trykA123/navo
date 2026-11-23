use std::rc::Rc;

use gpui_kit::{App, Div, Entity, FocusHandle, InteractiveElement, ScrollHandle, Stateful, Window};

use super::options::PickHandler;
use super::{ConfirmDialog, DialogState, Handler};
use crate::dialog_focus::{self, Next, NextButton, Press, Previous, PreviousButton};

const KEY_CONTEXT: &str = "ConfirmDialog Dialog";

pub(super) const BUTTON: &str = "ConfirmButton DialogButton";

gpui_kit::actions!(confirm_dialog, [Dismiss]);

pub(super) struct Keys {
    pub stops: Vec<FocusHandle>,
    pub buttons: Vec<FocusHandle>,
    pub confirm: FocusHandle,
    pub can_confirm: bool,
    pub cancel: FocusHandle,
    pub choice: Option<FocusHandle>,
    pub switch: Option<FocusHandle>,
    /// The options row's focus, when the dialog has one.
    pub options: Option<FocusHandle>,
    pub on_confirm: Option<Handler>,
    pub on_cancel: Option<Handler>,
    pub on_toggle: Option<Handler>,
    pub on_switch: Option<Handler>,
    pub on_pick: Option<PickHandler>,
    pub details: DetailKeys,
}

/// The details list's keys: whether it is open, its link, list and Back stops, and the handlers
/// that open and close it.
#[derive(Default)]
pub(super) struct DetailKeys {
    pub open: bool,
    pub link: Option<FocusHandle>,
    pub list: Option<FocusHandle>,
    pub back: Option<FocusHandle>,
    pub on_open: Option<Handler>,
    pub on_close: Option<Handler>,
    pub scroll: ScrollHandle,
}

impl Keys {
    /// Esc and Cancel: closes the details list when it is open, else cancels the dialog.
    pub(super) fn cancel(&self, window: &mut Window, cx: &mut App) {
        let close = self.details.on_close.as_ref().filter(|_| self.details.open);
        if let Some(handler) = close.or(self.on_cancel.as_ref()) {
            handler(window, cx);
        }
    }

    pub(super) fn confirm(&self, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_confirm {
            handler(window, cx);
        }
    }

    pub(super) fn toggle(&self, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_toggle {
            handler(window, cx);
        }
    }

    /// Runs the switch row's handler, if the dialog has one.
    pub(super) fn switch(&self, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_switch {
            handler(window, cx);
        }
    }

    /// Presses the focused stop: a check row flips and a button confirms or cancels. On the options
    /// row the press goes on to the row's own key listener, which confirms on Enter only; returns
    /// false then.
    fn activate(&self, window: &mut Window, cx: &mut App) -> bool {
        let has_focus = |handle: &Option<FocusHandle>| {
            handle
                .as_ref()
                .is_some_and(|h| h.contains_focused(window, cx))
        };
        if has_focus(&self.choice) {
            self.toggle(window, cx);
        } else if has_focus(&self.switch) {
            self.switch(window, cx);
        } else if has_focus(&self.options) {
            return false;
        } else if has_focus(&self.details.link) {
            run(self.details.on_open.as_ref(), window, cx);
        } else if has_focus(&self.details.back) {
            run(self.details.on_close.as_ref(), window, cx);
        } else if self.cancel.contains_focused(window, cx) {
            self.cancel(window, cx);
        } else if self.can_confirm && self.confirm.contains_focused(window, cx) {
            self.confirm(window, cx);
        }
        true
    }
}

/// The dialog's keys: its Tab stops in order, its buttons and its handlers. While the details
/// list is open, only the list and its Back button are stops.
/// The details list's keys for `state`; `link` is the link's focus when the dialog has details.
fn detail_keys(
    state: &Entity<DialogState>,
    link: Option<FocusHandle>,
    open: bool,
    current: &DialogState,
) -> DetailKeys {
    let has = link.is_some();
    DetailKeys {
        open,
        on_open: has.then(|| super::details::show(state, true)),
        on_close: has.then(|| super::details::show(state, false)),
        link,
        list: Some(current.list.clone()),
        back: Some(current.back.clone()),
        scroll: current.list_scroll.clone(),
    }
}

pub(super) fn keys_for(
    dialog: ConfirmDialog,
    state: &Entity<DialogState>,
    can_confirm: bool,
    cx: &App,
) -> Keys {
    let current = state.read(cx);
    let open = current.details_open && dialog.details.is_some();
    let confirm = can_confirm.then(|| current.confirm.clone());
    let buttons: Vec<FocusHandle> = confirm
        .into_iter()
        .chain([current.cancel.clone()])
        .collect();
    let choice = dialog
        .props
        .choice
        .is_some()
        .then(|| current.choice.clone());
    let switch = dialog.switch.is_some().then(|| current.switch.clone());
    let options = dialog.options.is_some().then(|| current.options.clone());
    let link = dialog.details.is_some().then(|| current.link.clone());
    let (stops, buttons) = if open {
        let back = current.back.clone();
        (vec![current.list.clone(), back.clone()], vec![back])
    } else {
        let stops = current
            .field_focus(cx)
            .into_iter()
            .chain(options.clone())
            .chain(link.clone())
            .chain(choice.clone())
            .chain(switch.clone())
            .chain(buttons.clone())
            .collect();
        (stops, buttons)
    };
    Keys {
        stops,
        buttons,
        confirm: current.confirm.clone(),
        can_confirm,
        cancel: current.cancel.clone(),
        choice,
        switch,
        options,
        details: detail_keys(state, link, open, current),
        on_confirm: dialog.on_confirm,
        on_cancel: dialog.on_cancel,
        on_toggle: dialog.on_toggle,
        on_switch: dialog.on_switch,
        on_pick: dialog.on_pick,
    }
}

/// Runs `handler` when there is one.
fn run(handler: Option<&Handler>, window: &mut Window, cx: &mut App) {
    if let Some(handler) = handler {
        handler(window, cx);
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
        .on_action(move |_: &Press, window, cx| {
            if activate.activate(window, cx) {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        })
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
