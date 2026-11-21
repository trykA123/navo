//! Undo of the last copy (NC148): Ctrl+Z or the toast's Undo asks the app to revert it, the
//! screen reloads what the undo touched, and the window hears `ScreenEvent::Wrote`.

use std::sync::PoisonError;

use gpui_kit::Stateful;
use navo_core::Location;
use navo_l10n::tr;

use super::{CopyState, Note};
use crate::compare_diff::{CopyResult, UndoDone, UndoSink, UndoStart};
use crate::list_actions::UndoLast;
use crate::prelude::*;
use crate::window_view::{DiffSlot, ScreenEvent, ScreenOwner};

impl DiffSlot {
    /// A sink that queues the end of an undo and wakes the window.
    fn undo_sink(&self) -> UndoSink {
        let (inbox, wake) = (self.copy.undo_inbox.clone(), self.wake.clone());
        std::sync::Arc::new(move |done| {
            inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(done);
            wake();
        })
    }
}

/// The note for the end of an undo: "Undid the copy", or the refusal text. It has no Undo.
fn note_of(result: &CopyResult) -> Note {
    let (text, error) = match result {
        CopyResult::Copied => (tr("compare-undone"), false),
        CopyResult::Refused(text) | CopyResult::Unconfirmed(text) => (text.clone(), true),
    };
    Note {
        text: text.into(),
        undo: false,
        error,
    }
}

impl ScreenOwner {
    /// Binds the `UndoLast` action (Ctrl+Z) on a screen's `root`; with nothing to undo the
    /// action passes on to the next handler.
    pub(super) fn undo_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &UndoLast, _, cx| {
            if view.start_undo(None, cx) == UndoStart::Nothing {
                cx.propagate();
            }
        }))
    }

    /// Undoes copy `job` for the toast's Undo, only while it is the newest copy (NC166).
    pub fn undo_write(&mut self, job: &str, cx: &mut Context<Self>) -> UndoStart {
        self.start_undo(Some(job), cx)
    }

    /// Asks the app to undo the newest copy, only when it is `job` (any for `None`). While a
    /// copy or an undo runs, says so in the status line and answers `Started`.
    pub(super) fn start_undo(&mut self, job: Option<&str>, cx: &mut Context<Self>) -> UndoStart {
        let copy = &mut self.compare.diff.copy;
        if copy.inflight {
            copy.note = Some(Note {
                text: tr("compare-copy-running").into(),
                undo: false,
                error: false,
            });
            cx.notify();
            return UndoStart::Started;
        }
        let Some(hooks) = self.compare.diff.hooks.clone() else {
            return UndoStart::Nothing;
        };
        let sink = self.compare.diff.undo_sink();
        let start = hooks.undo_last(job, sink);
        if start == UndoStart::Started {
            let copy = &mut self.compare.diff.copy;
            (copy.inflight, copy.note) = (true, None);
            cx.notify();
        }
        start
    }

    /// Takes the ends of undos. Returns whether anything arrived.
    pub(super) fn poll_undos(&mut self, cx: &mut Context<Self>) -> bool {
        let done = std::mem::take(&mut *lock(&self.compare.diff.copy));
        for done in &done {
            self.finish_undo(done, cx);
        }
        !done.is_empty()
    }

    /// Shows the note for `done`, reloads the open file and the Stream cards that show a
    /// target, and tells the window.
    fn finish_undo(&mut self, done: &UndoDone, cx: &mut Context<Self>) {
        let copy = &mut self.compare.diff.copy;
        (copy.inflight, copy.hover, copy.removing) = (false, None, false);
        let note = note_of(&done.result);
        let text = note.text.to_string();
        copy.note = Some(note);
        let reload = if done.result.may_have_written() {
            done.targets.clone()
        } else {
            Vec::new()
        };
        self.reload_targets(&reload);
        cx.emit(ScreenEvent::Wrote {
            text,
            job: None,
            reload,
        });
        cx.notify();
    }

    /// Reloads what shows any of `targets`, after another screen's undo rewrote them.
    pub fn reload_written(&mut self, targets: &[Location], cx: &mut Context<Self>) {
        self.reload_targets(targets);
        cx.notify();
    }

    /// Reloads the file screen and the Stream items whose left or right side is in `targets`.
    fn reload_targets(&mut self, targets: &[Location]) {
        let shows = |left: &Option<Location>, right: &Option<Location>| {
            [left, right]
                .into_iter()
                .flatten()
                .any(|place| targets.contains(place))
        };
        let Some(screen) = self.compare.screen.as_mut() else {
            return;
        };
        if let Some(file) = screen
            .file
            .as_mut()
            .filter(|f| shows(&f.request.left, &f.request.right))
        {
            file.at = None;
            file.refresh();
        }
        if let Some(stream) = screen.stream.as_mut() {
            let ids: Vec<u64> = stream
                .requests
                .iter()
                .filter(|(_, pair)| shows(&pair.left, &pair.right))
                .map(|(id, _)| *id)
                .collect();
            for id in ids {
                stream.reload_item(id);
            }
        }
    }
}

/// The queued ends of undos, locked.
fn lock(copy: &CopyState) -> std::sync::MutexGuard<'_, Vec<UndoDone>> {
    copy.undo_inbox
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
