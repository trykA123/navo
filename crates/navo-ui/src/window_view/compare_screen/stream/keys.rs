//! The Stream's keys (D27): J/K and Shift+J/K move, V marks Viewed, O folds, Esc steps back.
//! Each move reveals the current stop and, once scrolling rests, tells the app what is in view.

use std::time::Duration;

use gpui_kit::Stateful;

use super::StreamScreen;
use crate::commands::compare::{
    Exit, Fold, NextDifference, NextFile, PreviousDifference, PreviousFile, ToggleViewed,
};
use crate::compare_diff::EscStep;
use crate::prelude::*;
use crate::window_view::NavoWindow;

/// How long a scroll rests before the app hears which cards are in view.
const SETTLE: Duration = Duration::from_millis(100);

impl NavoWindow {
    /// Binds the Stream's keys on `root`: J/K, Shift+J/K, V, O and Esc (D27).
    pub(super) fn stream_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let step = |change: i32, file: bool| {
            move |stream: &mut StreamScreen| match file {
                true => stream.model.next_file(change),
                false => stream.model.next_change(change),
            }
        };
        root.on_action(cx.listener(move |view, _: &NextDifference, _, cx| {
            view.stream_step(cx, step(1, false));
        }))
        .on_action(cx.listener(move |view, _: &PreviousDifference, _, cx| {
            view.stream_step(cx, step(-1, false));
        }))
        .on_action(cx.listener(move |view, _: &NextFile, _, cx| {
            view.stream_step(cx, step(1, true));
        }))
        .on_action(cx.listener(move |view, _: &PreviousFile, _, cx| {
            view.stream_step(cx, step(-1, true));
        }))
        .on_action(cx.listener(|view, _: &ToggleViewed, _, cx| {
            view.stream_toggle_viewed(cx);
        }))
        .on_action(cx.listener(|view, _: &Fold, _, cx| {
            view.stream_step(cx, |stream| {
                stream.model.toggle_fold();
                true
            });
        }))
        .on_action(cx.listener(|view, _: &Exit, window, cx| view.stream_exit(window, cx)))
    }

    /// Sends the focus once scrolling rests for 100 ms; a newer scroll restarts the wait.
    pub(in crate::window_view::compare_screen) fn settle_stream_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let _ = this.update(cx, Self::stream_settled);
        });
        if let Some(stream) = self.stream_mut() {
            stream.settle = Some(task);
        }
    }

    /// The scroll rested: sends the focus and redraws when far cards were dropped.
    fn stream_settled(&mut self, cx: &mut Context<Self>) {
        if self.stream_mut().is_some_and(StreamScreen::send_focus) {
            cx.notify();
        }
    }

    /// Runs one key's `step` on the Stream; when it changed something, rebuilds the rows,
    /// reveals the current stop and schedules the focus.
    fn stream_step(
        &mut self,
        cx: &mut Context<Self>,
        step: impl FnOnce(&mut StreamScreen) -> bool,
    ) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        if !step(stream) {
            return;
        }
        stream.sync_rows();
        stream.reveal_cursor();
        self.compare.diff.copy.note = None;
        self.settle_stream_focus(cx);
        cx.notify();
    }

    /// V: flips the current file's Viewed mark and stores it through the app.
    fn stream_toggle_viewed(&mut self, cx: &mut Context<Self>) {
        self.stream_step(cx, |stream| match stream.model.toggle_viewed() {
            Some((id, key, viewed)) => {
                stream.control.set_viewed(id, key, viewed);
                true
            }
            None => false,
        });
    }

    /// Esc on the Stream: closes the details drawer, clears the search, then closes the
    /// outline, then leaves to the panes.
    fn stream_exit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        if std::mem::take(&mut stream.details) {
            cx.notify();
        } else if stream.search_open() {
            self.search_close(window, cx);
        } else if stream.model.esc() == EscStep::Leave {
            self.leave_stream(window, cx);
        } else {
            stream.sync_rows();
            let focus = stream.focus.clone();
            window.focus(&focus, cx);
            cx.notify();
        }
    }

    /// Closes the Stream and gives focus back to the compare pane, with the current card's
    /// file revealed and selected there.
    pub(super) fn leave_stream(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.stream_mut().and_then(|stream| stream.current_path());
        self.compare.screen = None;
        self.compare.diff.copy.note = None;
        if let Some(path) = path {
            self.browser.compare_reveal(&path);
        }
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}
