//! Copy one change from the file diff or the Stream (V27, D29): Ctrl+Alt+←/→ or the hover bar
//! send the change to the app, the pair reloads, and the status line says what happened.

mod view;

use std::sync::{Arc, Mutex, PoisonError};

use gpui_kit::Stateful;
use navo_l10n::tr;

use super::{DiffSlot, FileScreen};
use crate::commands::compare::{CopyLeft, CopyRight};
use crate::compare_diff::{
    CopyChange, CopyDirection, CopyDone, CopyResult, CopySink, HunkView, PairKind, PairRequest,
};
use crate::prelude::*;
use crate::window_view::NavoWindow;

pub(super) use view::{FileBars, Note, note_view};

/// The change the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) enum Hover {
    /// Change number `0` of the open file diff.
    File(u32),
    /// Change `hunk` of the Stream card for item `item` (an item id).
    Card { item: u64, hunk: u32 },
}

/// Copy state of the window: the hovered change, the copy in flight and the last note.
#[derive(Default)]
pub(super) struct CopyState {
    /// Ends of copies, queued by the app's threads.
    inbox: Arc<Mutex<Vec<CopyDone>>>,
    hover: Option<Hover>,
    pub(super) inflight: bool,
    /// The change to make current once the file diff reloads.
    reopen_at: Option<usize>,
    /// Whether the copy in flight removes picked lines (its note says "Removed").
    removing: bool,
    pub(super) note: Option<Note>,
}

/// The copy a key or a click asks for, resolved to the app's changes: one, or one per change
/// the picked lines touch. With the change to make current after the reload.
pub(super) type Resolved = Result<(Vec<CopyChange>, Option<usize>), &'static str>;

/// The request for `hunk` of `pair`, or the Fluent id of why it cannot be made: no change
/// picked, or a hunk with no digest (coarse, paged or over the line limit).
pub(super) fn change_of(
    pair: &PairRequest,
    hunk: Option<&HunkView>,
    direction: CopyDirection,
) -> Result<CopyChange, &'static str> {
    let hunk = hunk.ok_or("compare-copy-pick")?;
    let digest = hunk.digest.ok_or("compare-copy-no-digest")?;
    Ok(CopyChange {
        pair: pair.clone(),
        left_bytes: hunk.left_bytes.clone(),
        right_bytes: hunk.right_bytes.clone(),
        digest,
        direction,
        lines: None,
    })
}

/// The note for the end of a copy: "Copied to the right" (or "Removed from the left") with
/// Undo, or the refusal text.
fn note_of(done: &CopyDone, removing: bool) -> Note {
    match &done.result {
        CopyResult::Copied => Note {
            text: tr(match (done.direction, removing) {
                (CopyDirection::ToLeft, false) => "compare-copied-left",
                (CopyDirection::ToRight, false) => "compare-copied-right",
                (CopyDirection::ToLeft, true) => "compare-removed-left",
                (CopyDirection::ToRight, true) => "compare-removed-right",
            })
            .into(),
            undo: true,
            error: false,
        },
        CopyResult::Refused(text) => Note {
            text: text.clone().into(),
            undo: false,
            error: true,
        },
    }
}

impl DiffSlot {
    /// A sink that queues the end of a copy and wakes the window.
    fn copy_sink(&self) -> CopySink {
        let (inbox, wake) = (Arc::clone(&self.copy.inbox), Arc::clone(&self.wake));
        Arc::new(move |done| {
            inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(done);
            wake();
        })
    }
}

impl FileScreen {
    /// The copy of change `hunk` of the open pair; with no `hunk`, the picked lines when any
    /// are picked, else the current change.
    fn copy_request(&self, hunk: Option<usize>, direction: CopyDirection) -> Resolved {
        let view = self.view.as_ref().ok_or("compare-copy-not-copyable")?;
        if hunk.is_none() && self.pick.has_lines() {
            let changes = self.pick.changes(view.text(), &self.request, direction)?;
            return Ok((changes, view.current()));
        }
        let at = hunk.or(view.current());
        let found = at.and_then(|at| view.text().hunks.get(at));
        Ok((vec![change_of(&self.request, found, direction)?], at))
    }
}

impl NavoWindow {
    /// Binds Ctrl+Alt+← and Ctrl+Alt+→ (copy the current change) on a screen's `root`.
    pub(super) fn copy_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &CopyLeft, _, cx| {
            view.copy_change_to(CopyDirection::ToLeft, None, cx);
        }))
        .on_action(cx.listener(|view, _: &CopyRight, _, cx| {
            view.copy_change_to(CopyDirection::ToRight, None, cx);
        }))
    }

    /// Copies `target` (else the current change) of the screen on top in `direction`. A change
    /// that cannot be copied, or none picked, says why in the status line.
    pub(super) fn copy_change_to(
        &mut self,
        direction: CopyDirection,
        target: Option<Hover>,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.resolve_copy(direction, target);
        self.start_copy(resolved, false, cx);
    }

    /// Sends the resolved copy to the app, or says in the status line why it cannot go. The
    /// picks are spent when the copy goes, kept when it cannot be resolved. While a copy or a
    /// sheet runs, says so. `removing` makes the end say "Removed".
    pub(super) fn start_copy(
        &mut self,
        resolved: Resolved,
        removing: bool,
        cx: &mut Context<Self>,
    ) {
        if self.compare.diff.copy.inflight {
            self.compare.diff.copy.note = Some(Note {
                text: tr("compare-copy-running").into(),
                undo: false,
                error: false,
            });
            cx.notify();
            return;
        }
        let started = resolved.and_then(|(changes, at)| {
            let hooks = self.compare.diff.hooks.clone();
            hooks
                .map(|hooks| (hooks, changes, at))
                .ok_or("compare-copy-not-copyable")
        });
        match started {
            Ok((hooks, changes, at)) => {
                self.clear_picks();
                let sink = self.compare.diff.copy_sink();
                let copy = &mut self.compare.diff.copy;
                (copy.inflight, copy.reopen_at, copy.note) = (true, at, None);
                copy.removing = removing;
                hooks.copy_changes(changes, sink);
            }
            Err(why) => {
                self.compare.diff.copy.note = Some(Note {
                    text: tr(why).into(),
                    undo: false,
                    error: true,
                });
            }
        }
        cx.notify();
    }

    /// The request for the file diff when one is open, else for the Stream's card.
    fn resolve_copy(&self, direction: CopyDirection, target: Option<Hover>) -> Resolved {
        let screen = self.compare.screen.as_ref().ok_or("compare-copy-pick")?;
        if let Some(file) = &screen.file {
            let hunk = match target {
                Some(Hover::File(hunk)) => Some(hunk as usize),
                _ => None,
            };
            return file.copy_request(hunk, direction);
        }
        let stream = screen.stream.as_ref().ok_or("compare-copy-pick")?;
        let stop = stream.model.cursor();
        let (id, hunk) = match target {
            Some(Hover::Card { item, hunk }) => (item, Some(hunk)),
            _ => (
                stream
                    .model
                    .items()
                    .get(stop.item)
                    .ok_or("compare-copy-pick")?
                    .id,
                stop.hunk,
            ),
        };
        let card = stream.model.card(id).ok_or("compare-copy-not-copyable")?;
        if card.kind != PairKind::Text {
            return Err("compare-copy-not-copyable");
        }
        let pair = stream
            .requests
            .get(&id)
            .ok_or("compare-copy-not-copyable")?;
        let found = hunk.and_then(|hunk| card.hunks.get(hunk as usize));
        Ok((vec![change_of(pair, found, direction)?], None))
    }

    /// The pointer entered (`on`) or left `hover`; the bar shows on the entered change.
    pub(super) fn set_copy_hover(&mut self, hover: Hover, on: bool, cx: &mut Context<Self>) {
        let state = &mut self.compare.diff.copy;
        let next = match on {
            true => Some(hover),
            false if state.hover == Some(hover) => None,
            false => return,
        };
        if state.hover != next {
            state.hover = next;
            cx.notify();
        }
    }

    /// Takes the ends of copies. A copied pair and its Stream card reload, so they show the
    /// files as they are now; a refusal reloads nothing. A copied pair opens on the change that
    /// took the copied one's place.
    /// Returns whether anything arrived.
    pub(super) fn poll_copies(&mut self) -> bool {
        let done = std::mem::take(
            &mut *self
                .compare
                .diff
                .copy
                .inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for done in &done {
            self.finish_copy(done);
        }
        !done.is_empty()
    }

    /// Shows the note for `done` and, when it copied, reloads the one pair it touched.
    fn finish_copy(&mut self, done: &CopyDone) {
        let copy = &mut self.compare.diff.copy;
        (copy.inflight, copy.hover) = (false, None);
        copy.note = Some(note_of(done, std::mem::take(&mut copy.removing)));
        let at = copy.reopen_at.take();
        let Some(screen) = self.compare.screen.as_mut() else {
            return;
        };
        if let Some(file) = screen
            .file
            .as_mut()
            .filter(|f| f.request.id == done.request && done.result == CopyResult::Copied)
        {
            file.at = at;
            file.control.refresh();
        }
        if let (Some(stream), CopyResult::Copied) = (screen.stream.as_mut(), &done.result) {
            stream.reload_item(done.request);
        }
    }

    /// The hovered change of the file diff.
    pub(super) fn file_hover(&self) -> Option<u32> {
        match self.compare.diff.copy.hover {
            Some(Hover::File(hunk)) => Some(hunk),
            _ => None,
        }
    }
}

#[cfg(test)]
impl NavoWindow {
    /// The last copy's note as text, whether it has Undo and whether it is an error.
    pub(in crate::window_view) fn copy_note(&self) -> Option<(String, bool, bool)> {
        let note = self.compare.diff.copy.note.as_ref()?;
        Some((note.text.to_string(), note.undo, note.error))
    }
}
