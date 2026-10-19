use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Step};

use super::aside::presence;
use super::copy::{remove_if_present, temp_of};
use super::restore::{Restored, restore, surely_absent};
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
        let logged = match undo(job, step, held, ctx) {
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

pub(super) fn held_for<'h>(step: &Step, held: &'h [HeldFile]) -> Option<&'h HeldFile> {
    let Step::CopyFile { target, .. } = step else {
        return None;
    };
    held.iter()
        .find(|file| file.original == *target)
        .or_else(|| held.iter().find(|file| file.original.same_place(target)))
}

fn undo(
    job: &Journaled<'_>,
    step: &Step,
    held: Option<&HeldFile>,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    let ops = ctx.ops;
    match step {
        Step::CopyFile { source, target, .. } => {
            if source_gone(job, source, ops)? {
                return Ok(StepOutcome::Skipped);
            }
            match held {
                Some(held) => put_back(held, ctx),
                None => already_undone(ops.remove_file(target)),
            }
        }
        Step::MakeFolder { target } => Ok(match ops.remove_empty_folder(target) {
            Ok(()) => StepOutcome::Done,
            Err(_) => StepOutcome::Skipped,
        }),
        Step::Rename { source, target } => {
            if surely_absent(ops, target)? && presence(ops, source)?.is_some() {
                return Ok(StepOutcome::Skipped);
            }
            ops.rename(target, source).map(|()| StepOutcome::Done)
        }
        Step::Recycle { .. }
        | Step::RemoveFile { .. }
        | Step::RemoveLink { .. }
        | Step::RemoveFolder { .. } => Ok(StepOutcome::Skipped),
    }
}

fn already_undone(removed: Result<(), FsError>) -> Result<StepOutcome, FsError> {
    match removed {
        Ok(()) => Ok(StepOutcome::Done),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(StepOutcome::Skipped),
        Err(error) => Err(error),
    }
}

fn source_gone(
    job: &Journaled<'_>,
    source: &Location,
    ops: &dyn ProviderOps,
) -> Result<bool, FsError> {
    let removed_by_job = (0_u32..).zip(&job.plan.steps).any(|(seq, step)| {
        matches!(step, Step::RemoveFile { location } if location.same_place(source))
            && job.done.contains(&seq)
    });
    if removed_by_job {
        return Ok(true);
    }
    surely_absent(ops, source)
}

fn put_back(held: &HeldFile, ctx: &mut JobContext<'_>) -> Result<StepOutcome, FsError> {
    Ok(match restore(ctx.ops, held)? {
        Restored::Back => StepOutcome::Done,
        Restored::Beside(beside) => {
            ctx.log.kept_beside(held, &beside);
            StepOutcome::Done
        }
        Restored::Gone => StepOutcome::Skipped,
    })
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
