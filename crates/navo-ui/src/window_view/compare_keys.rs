use gpui_kit::Div;
use navo_core::CursorMove;

use super::NavoWindow;
use crate::browser::Browser;
use crate::commands::compare::{
    Continue, Enter, Exit, NextDifference, PreviousDifference, StopChecking, Up, WithOtherPane,
};
use crate::file_list::{
    CursorDown, CursorFirst, CursorLast, CursorUp, PageDown, PageUp, SwitchPane,
};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "compare");
}

/// The compare commands on a pane; each one works only in compare mode except the start.
pub(super) fn compare_keys(root: Div, cx: &mut Context<NavoWindow>) -> Div {
    root.on_action(
        cx.listener(|this: &mut NavoWindow, _: &WithOtherPane, window, cx| {
            this.compare_with_other_pane(window, cx);
        }),
    )
    .on_action(step::<Enter>(Browser::compare_enter, cx))
    .on_action(step::<Up>(Browser::compare_up, cx))
    .on_action(step::<Exit>(Browser::compare_exit, cx))
    .on_action(step::<NextDifference>(Browser::compare_next_difference, cx))
    .on_action(step::<PreviousDifference>(
        Browser::compare_previous_difference,
        cx,
    ))
    .on_action(step::<StopChecking>(Browser::compare_stop_checking, cx))
    .on_action(step::<Continue>(Browser::compare_continue, cx))
    .on_action(cursor::<CursorUp>(CursorMove::Up, cx))
    .on_action(cursor::<CursorDown>(CursorMove::Down, cx))
    .on_action(cursor::<PageUp>(CursorMove::PageUp, cx))
    .on_action(cursor::<PageDown>(CursorMove::PageDown, cx))
    .on_action(cursor::<CursorFirst>(CursorMove::First, cx))
    .on_action(cursor::<CursorLast>(CursorMove::Last, cx))
    .on_action(
        cx.listener(|this: &mut NavoWindow, _: &SwitchPane, window, cx| {
            if this.browser.compare_mode().is_some() {
                this.switch_pane(window, cx);
            }
        }),
    )
}

/// Cursor keys move the compare selection; the list's own keys never reach here in a folder pane.
fn cursor<A: gpui_kit::Action>(
    to: CursorMove,
    cx: &mut Context<NavoWindow>,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    step(move |browser: &mut Browser| browser.compare_cursor(to), cx)
}

fn step<A: gpui_kit::Action>(
    run: impl Fn(&mut Browser) -> bool + 'static,
    cx: &mut Context<NavoWindow>,
) -> impl Fn(&A, &mut Window, &mut App) + 'static {
    cx.listener(move |this: &mut NavoWindow, _: &A, window, cx| {
        if run(&mut this.browser) {
            this.focus_core_pane(window, cx);
            cx.notify();
        }
    })
}

impl NavoWindow {
    fn compare_with_other_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(refused) = self.browser.compare_with_other_pane(&self.values) {
            self.show_notice(StatusNotice {
                text: navo_l10n::tr(refused.fluent_id()).into(),
                is_error: true,
            });
        }
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    /// Shows the notice of a compare that failed to start.
    pub(super) fn show_compare_notice(&mut self) -> bool {
        let Some(text) = self.browser.take_compare_notice() else {
            return false;
        };
        self.show_notice(StatusNotice {
            text: text.into(),
            is_error: true,
        });
        true
    }
}
