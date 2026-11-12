//! The plan sheet for "copy all matches" (V31, D31): Ctrl+Shift+←/→ or the "Copy all N" button
//! asks the app for a dry run, the sheet lists each file and its refusals, Enter runs one job and
//! Esc cancels. One Ctrl+Z then undoes every file.

use std::sync::{Arc, Mutex, PoisonError};

use gpui_kit::Stateful;
use navo_l10n::{t, tr};

mod rows;
mod view;

use super::copy::Note;
use crate::commands::compare::{CopyAllLeft, CopyAllRight};
use crate::compare_diff::{
    CopyDirection, SheetPlan, SheetPlanSink, SheetResult, SheetRow, SheetSink,
};
use crate::format;
use crate::prelude::*;
use crate::window_view::ScreenOwner;
use rows::{gather, ordered, totals};
pub(super) use view::buttons;

/// What the app sent back for the sheet, waiting for the next poll.
enum Event {
    Planned(SheetPlan),
    Done(SheetResult),
}

/// Where the sheet is in its life.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// The app is planning; `local` are the rows the UI refused before asking.
    Planning {
        direction: CopyDirection,
        local: Vec<SheetRow>,
    },
    /// The sheet is open on `rows`, refused ones first.
    Asking {
        direction: CopyDirection,
        rows: Vec<SheetRow>,
    },
    /// The one job runs; `changes` is what the plan promised.
    Running {
        direction: CopyDirection,
        changes: usize,
    },
}

/// The sheet of the window: its phase and the app's answers.
#[derive(Default)]
pub(super) struct SheetState {
    inbox: Arc<Mutex<Vec<Event>>>,
    phase: Phase,
    /// How many sheets were asked; gives each dialog its own element id.
    asks: u64,
}

impl SheetState {
    /// Whether a plan or a job is in flight or the sheet is open.
    pub(super) fn busy(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }
}

impl ScreenOwner {
    /// Binds Ctrl+Shift+← and Ctrl+Shift+→ (copy all matches) on the Stream's `root`.
    pub(super) fn sheet_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &CopyAllLeft, _, cx| {
            view.sheet_open(CopyDirection::ToLeft, cx);
        }))
        .on_action(cx.listener(|view, _: &CopyAllRight, _, cx| {
            view.sheet_open(CopyDirection::ToRight, cx);
        }))
    }

    /// Puts an error `id` in the status line.
    fn sheet_say(&mut self, id: &str, cx: &mut Context<Self>) {
        self.compare.diff.copy.note = Some(Note {
            text: tr(id).into(),
            undo: false,
            error: true,
        });
        cx.notify();
    }

    /// The sinks that queue the app's plan and job end, and wake the window.
    fn sheet_sinks(&self) -> (SheetPlanSink, SheetSink) {
        let (inbox, wake) = (
            Arc::clone(&self.compare.diff.sheet.inbox),
            Arc::clone(&self.compare.diff.wake),
        );
        let (planned_inbox, planned_wake) = (Arc::clone(&inbox), Arc::clone(&wake));
        let planned: SheetPlanSink = Arc::new(move |plan| {
            let mut queue = planned_inbox.lock().unwrap_or_else(PoisonError::into_inner);
            queue.push(Event::Planned(plan));
            planned_wake();
        });
        let done: SheetSink = Arc::new(move |result| {
            let mut queue = inbox.lock().unwrap_or_else(PoisonError::into_inner);
            queue.push(Event::Done(result));
            wake();
        });
        (planned, done)
    }

    /// Ctrl+Shift+←/→ or the button: with a finished search, asks the app to plan the copy of
    /// every matched change; otherwise says why not.
    pub(super) fn sheet_open(&mut self, direction: CopyDirection, cx: &mut Context<Self>) {
        if self.compare.diff.sheet.busy() || self.compare.diff.copy.inflight {
            return;
        }
        let Some(stream) = self.stream_mut() else {
            return;
        };
        let model = &stream.model;
        let refusal = if !model.searching() {
            Some("compare-sheet-need-search")
        } else if !model.search_done() {
            Some("compare-sheet-wait")
        } else if model.match_counts().0 == 0 {
            Some("compare-sheet-none")
        } else {
            None
        };
        if let Some(id) = refusal {
            return self.sheet_say(id, cx);
        }
        let (changes, local) = gather(stream, direction);
        let hooks = self.compare.diff.hooks.clone();
        let Some(hooks) = hooks else {
            return self.sheet_say("compare-copy-not-copyable", cx);
        };
        let sheet = &mut self.compare.diff.sheet;
        sheet.phase = Phase::Planning { direction, local };
        self.compare.diff.copy.note = None;
        self.sheet_hold();
        let (planned, _) = self.sheet_sinks();
        if changes.is_empty() {
            planned(SheetPlan::default());
        } else {
            hooks.plan_sheet(changes, planned);
        }
        cx.notify();
    }

    /// While a sheet is planned, open or running, single copies wait: the copy flag follows it.
    fn sheet_hold(&mut self) {
        self.compare.diff.copy.inflight = self.compare.diff.sheet.busy();
    }

    /// Takes the app's answers: a plan opens the sheet, the end of the job shows its note and
    /// reloads the Stream. Returns whether anything arrived.
    pub(super) fn poll_sheet(&mut self) -> bool {
        let events = std::mem::take(
            &mut *self
                .compare
                .diff
                .sheet
                .inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for event in &events {
            match event {
                Event::Planned(plan) => self.sheet_planned(plan),
                Event::Done(result) => self.sheet_done(result),
            }
        }
        if !events.is_empty() {
            self.sheet_hold();
        }
        !events.is_empty()
    }

    /// The plan came: opens the sheet, or says nothing can be copied when it has no row.
    fn sheet_planned(&mut self, plan: &SheetPlan) {
        let sheet = &mut self.compare.diff.sheet;
        let Phase::Planning { direction, local } = std::mem::take(&mut sheet.phase) else {
            return;
        };
        let rows = ordered(plan.rows.clone(), local);
        if rows.is_empty() {
            self.compare.diff.copy.note = Some(Note {
                text: tr("compare-sheet-none").into(),
                undo: false,
                error: true,
            });
            return;
        }
        sheet.asks += 1;
        sheet.phase = Phase::Asking { direction, rows };
    }

    /// The job ended: shows "Copied …" with Undo, or the refusal, and reloads the Stream on success only.
    fn sheet_done(&mut self, result: &SheetResult) {
        let Phase::Running { direction, changes } =
            std::mem::take(&mut self.compare.diff.sheet.phase)
        else {
            return;
        };
        let note = match result {
            SheetResult::Copied { files, bytes } => Note {
                text: t!(
                    match direction {
                        CopyDirection::ToLeft => "compare-sheet-copied-left",
                        CopyDirection::ToRight => "compare-sheet-copied-right",
                    },
                    changes = changes,
                    files = *files,
                    size = format::size(*bytes)
                )
                .into(),
                undo: true,
                error: false,
            },
            SheetResult::Refused(text) => Note {
                text: text.clone().into(),
                undo: false,
                error: true,
            },
        };
        self.compare.diff.copy.note = Some(note);
        if !matches!(result, SheetResult::Copied { .. }) {
            return;
        }
        if let Some(stream) = self.stream_mut() {
            stream.reload();
        }
    }

    /// Enter (`confirmed`) runs the planned files as one job; Esc drops the sheet. Focus goes
    /// back to the cards either way.
    fn sheet_answer(&mut self, confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Phase::Asking { direction, rows } = std::mem::take(&mut self.compare.diff.sheet.phase)
        else {
            return;
        };
        if let Some(stream) = self.stream_mut() {
            let focus = stream.focus.clone();
            window.focus(&focus, cx);
        }
        let (changes, files, _) = totals(&rows);
        let hooks = self.compare.diff.hooks.clone();
        if let (true, true, Some(hooks)) = (confirmed, files > 0, hooks) {
            let (_, done) = self.sheet_sinks();
            self.compare.diff.sheet.phase = Phase::Running { direction, changes };
            hooks.run_sheet(done);
        }
        self.sheet_hold();
        cx.notify();
    }
}

#[cfg(test)]
impl ScreenOwner {
    /// The open sheet's lines, or `None` while it is closed.
    pub(in crate::window_view) fn sheet_lines(&self) -> Option<Vec<String>> {
        let Phase::Asking { direction, rows } = &self.compare.diff.sheet.phase else {
            return None;
        };
        Some(
            rows::props_of(*direction, rows)
                .lines
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
    }
}
