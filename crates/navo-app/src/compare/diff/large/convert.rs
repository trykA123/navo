//! Turns engine `DiffEvent`s into UI `DiffUpdate::Large` snapshots (V35).

use std::ops::Range;

use navo_compare::model::DiffStep;
use navo_compare::text::large::{DiffEvent, LargeDiff as Engine};
use navo_ui::compare_diff::{DiffUpdate, LargeProgress, LargeStage};

use super::state::{State, Step, progress_view};
use super::{Ctx, failure_text};

const DONE_PROGRESS: LargeProgress = LargeProgress {
    stage: LargeStage::Sections,
    done: 1,
    total: 1,
};

/// Apply one engine event and report the state; returns the focus to apply after `Done`.
pub(crate) fn apply(event: DiffEvent, state: &mut State, ctx: &Ctx) -> Option<u64> {
    match event {
        DiffEvent::Provisional { hunks, .. } => {
            state.hunks.clear();
            state.add_hunks(&hunks);
        }
        DiffEvent::Hunks(covered) => {
            state.drop_hunks(&covered.left, &covered.right);
            state.add_hunks(&covered.hunks);
        }
        DiffEvent::Step {
            step,
            section,
            lines,
            reason,
        } => state.add_step(Step {
            step,
            section,
            lines,
            reason,
        }),
        DiffEvent::Paged { paged, seq } => {
            state.drop_hunks(&paged.left, &paged.right);
            let lines = span(&paged.left).saturating_add(span(&paged.right));
            state.add_step(Step {
                step: DiffStep::PagedHunks,
                section: paged.left.clone(),
                lines,
                reason: navo_compare::text::large::StepReason::Paged,
            });
            state.acks.note_paged(seq);
        }
        DiffEvent::Progress(progress) => state.progress = Some(progress_view(progress)),
        DiffEvent::Restarted => state.restart(),
        DiffEvent::Done(result) => {
            state.done = true;
            state.progress = Some(DONE_PROGRESS);
            send(state, ctx);
            finish(result, ctx);
            return state.pending_focus.take();
        }
    }
    send(state, ctx);
    None
}

/// Apply a focus change; its evictions stay charged until the view acknowledges them.
pub(super) fn focus(line: u64, state: &mut State, engine: &Engine, ctx: &Ctx) {
    let evictions = engine.set_focus(line);
    for paged in evictions.paged() {
        state.drop_hunks(&paged.left, &paged.right);
    }
    if evictions.paged().is_empty() {
        evictions.acknowledge();
    } else {
        state.acks.hold(Box::new(move || evictions.acknowledge()));
    }
    send(state, ctx);
}

/// Send the current hunks, steps and progress as the next numbered snapshot.
pub(crate) fn send(state: &mut State, ctx: &Ctx) {
    let ack = state.acks.next_snapshot();
    (ctx.sink)(DiffUpdate::Large {
        request: ctx.id,
        hunks: state.hunks.clone(),
        steps: state.steps.clone(),
        progress: state.progress,
        ack,
    });
}

/// Report a `Done` failure other than a cancellation.
fn finish(
    result: Result<navo_compare::text::large::Summary, navo_compare::model::Reason>,
    ctx: &Ctx,
) {
    if let Err(reason) = result
        && reason != navo_compare::model::Reason::Stopped
    {
        (ctx.sink)(DiffUpdate::Failed {
            request: ctx.id,
            text: failure_text(reason),
        });
    }
}

/// A range's length in lines, saturating at the end.
fn span(range: &Range<u64>) -> u64 {
    range.end.saturating_sub(range.start)
}
