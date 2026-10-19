use rusty_core::{FsError, Location, OperationPlan, Step};

use super::aside::restore;
use super::copy::{remove_if_present, temp_of};
use super::{HeldFile, JobContext, JobOutcome, Journaled, StepOutcome};
use crate::ProviderOps;

pub(super) fn roll_back(job: &Journaled<'_>, ctx: &mut JobContext<'_>) -> JobOutcome {
    let steps: Vec<(u32, &Step)> = (0_u32..).zip(&job.plan.steps).collect();
    let mut first = None;
    for &(seq, step) in steps.iter().rev() {
        let held = held_for(step, job.held);
        if !job.done.contains(&seq) && held.is_none() {
            continue;
        }
        let logged = match undo(step, held, ctx.ops) {
            Ok(outcome) => ctx.log.step(seq, outcome),
            Err(error) => {
                let _ = ctx.log.step(seq, StepOutcome::Failed);
                Err(error)
            }
        };
        if let Err(error) = logged {
            first.get_or_insert(JobOutcome::Failed { seq, error });
        }
    }
    first.unwrap_or(JobOutcome::Done)
}

fn held_for<'h>(step: &Step, held: &'h [HeldFile]) -> Option<&'h HeldFile> {
    let Step::CopyFile { target, .. } = step else {
        return None;
    };
    held.iter().find(|file| file.original.same_place(target))
}

fn undo(
    step: &Step,
    held: Option<&HeldFile>,
    ops: &dyn ProviderOps,
) -> Result<StepOutcome, FsError> {
    match step {
        Step::CopyFile { source, target, .. } => undo_copy(source, target, held, ops),
        Step::MakeFolder { target } => Ok(match ops.remove_empty_folder(target) {
            Ok(()) => StepOutcome::Done,
            Err(_) => StepOutcome::Skipped,
        }),
        Step::Rename { source, target } => ops.rename(target, source).map(|()| StepOutcome::Done),
        Step::Recycle { .. }
        | Step::RemoveFile { .. }
        | Step::RemoveLink { .. }
        | Step::RemoveFolder { .. } => Ok(StepOutcome::Skipped),
    }
}

fn undo_copy(
    source: &Location,
    target: &Location,
    held: Option<&HeldFile>,
    ops: &dyn ProviderOps,
) -> Result<StepOutcome, FsError> {
    if ops.metadata(source).is_err() {
        return Ok(StepOutcome::Skipped);
    }
    match held {
        Some(held) => restore(ops, held),
        None => ops.remove_file(target).map(|()| StepOutcome::Done),
    }
}

pub(super) fn clean_up(
    plan: &OperationPlan,
    done: &[u32],
    ops: &dyn ProviderOps,
) -> Result<u32, FsError> {
    let mut removed = 0;
    for (seq, step) in (0_u32..).zip(&plan.steps) {
        if done.contains(&seq) {
            continue;
        }
        if let Step::CopyFile { target, .. } = step
            && remove_if_present(ops, &temp_of(target)?)?
        {
            removed += 1;
        }
    }
    Ok(removed)
}
