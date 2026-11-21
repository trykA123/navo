//! Keyboard order of the start screen: the stops Tab and Shift+Tab walk, and who holds focus.
//! It extends `StartScreen`; the tab cells' focus handles come from `StartTabs`.

use gpui_kit::FocusHandle;
use sidewise_kit::prelude::*;

use crate::start::{Side, StartScreen};

/// One keyboard stop of the start screen, in Tab (visual) order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Folder,
    File,
    Left,
    Right,
    Compare,
    Rule,
    UpdateFirst,
    UpdateSecond,
}

/// Every stop, in Tab order; Tab wraps from the last to the first.
const STOPS: [Stop; 8] = [
    Stop::Folder,
    Stop::File,
    Stop::Left,
    Stop::Right,
    Stop::Compare,
    Stop::Rule,
    Stop::UpdateFirst,
    Stop::UpdateSecond,
];

/// Whether the update button's handle, if drawn, holds the focus.
fn update_focused(handle: &Option<FocusHandle>, window: &Window) -> bool {
    handle.as_ref().is_some_and(|h| h.is_focused(window))
}

/// Focuses an update button's handle, if drawn.
fn focus_update(handle: Option<&FocusHandle>, window: &mut Window, cx: &mut App) {
    if let Some(handle) = handle {
        handle.focus(window, cx);
    }
}

/// The stop after (or, `back`, before) `from` in Tab order that `drawn` accepts, wrapping.
fn step(from: Stop, back: bool, drawn: impl Fn(Stop) -> bool) -> Stop {
    let len = STOPS.len();
    let mut at = STOPS.iter().position(|&s| s == from).unwrap_or(0);
    loop {
        at = (at + if back { len - 1 } else { 1 }) % len;
        if drawn(STOPS[at]) {
            return STOPS[at];
        }
    }
}

impl StartScreen {
    /// The stop that holds the focus, if any.
    pub(crate) fn focused_stop(&self, window: &Window, cx: &App) -> Option<Stop> {
        let tabs = self.tabs.read(cx);
        let handles = self.updates.read(cx).handles(cx);
        STOPS.into_iter().find(|&stop| match stop {
            Stop::Left => self.focused_side(window, cx) == Some(Side::Left),
            Stop::Right => self.focused_side(window, cx) == Some(Side::Right),
            Stop::UpdateFirst => update_focused(&handles[0], window),
            Stop::UpdateSecond => update_focused(&handles[1], window),
            _ => tabs.handle(stop).is_some_and(|h| h.is_focused(window)),
        })
    }

    /// Tab and Shift+Tab: the next or previous stop, wrapping; from outside the stops (the
    /// list) the left field.
    pub(crate) fn cycle_stop(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let handles = self.updates.read(cx).handles(cx);
        let drawn = |stop: Stop| match stop {
            Stop::UpdateFirst => handles[0].is_some(),
            Stop::UpdateSecond => handles[1].is_some(),
            _ => true,
        };
        let next = self
            .focused_stop(window, cx)
            .map_or(Stop::Left, |stop| step(stop, back, drawn));
        match next {
            Stop::Left => self.focus_side(Side::Left, window, cx),
            Stop::Right => self.focus_side(Side::Right, window, cx),
            Stop::UpdateFirst => focus_update(handles[0].as_ref(), window, cx),
            Stop::UpdateSecond => focus_update(handles[1].as_ref(), window, cx),
            _ => {
                if let Some(handle) = self.tabs.read(cx).handle(next).cloned() {
                    handle.focus(window, cx);
                }
            }
        }
    }
}
