use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Step};

use super::aside::presence;
use super::copy::{remove_if_present, temp_of};
use super::restore::{
    Restored, gone_or_folder_deleted, keep, matches_stamp, restore, surely_absent,
};
use super::{FileStamp, HeldFile, JobContext, JobOutcome, Journaled, PlacedFile, StepOutcome};
use crate::ProviderOps;

pub(super) fn roll_back(job: &Journaled<'_>, ctx: &mut JobContext<'_>) -> JobOutcome {
    let steps: Vec<(u32, &Step)> = (0_u32..).zip(&job.plan.steps).collect();
    let mut first = None;
    for &(seq, step) in steps.iter().rev() {
        let held = held_for(step, job.held);
        if !job.done.contains(&seq) && held.is_none() {
            continue;
        }
        if awaits_unreadable_row(job, step, held) {
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

fn awaits_unreadable_row(job: &Journaled<'_>, step: &Step, held: Option<&HeldFile>) -> bool {
    job.held_unreadable && held.is_none() && matches!(step, Step::CopyFile { .. })
}

fn placed_for(step: &Step, placed: &[PlacedFile]) -> Option<FileStamp> {
    let Step::CopyFile { target, .. } = step else {
        return None;
    };
    placed
        .iter()
        .find(|file| file.target == *target)
        .or_else(|| placed.iter().find(|file| file.target.same_place(target)))
        .map(|file| file.stamp)
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
            if source_gone(job, source, held, ops)? {
                return held.map_or(Ok(StepOutcome::Skipped), |held| {
                    settle(keep(ops, held)?, held, ctx)
                });
            }
            match held {
                Some(held) => settle(restore(ops, held)?, held, ctx),
                None => remove_copy(ops, target, placed_for(step, job.placed)),
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

fn remove_copy(
    ops: &dyn ProviderOps,
    target: &Location,
    stamp: Option<FileStamp>,
) -> Result<StepOutcome, FsError> {
    let Some(placed) = presence(ops, target)? else {
        return Ok(StepOutcome::Skipped);
    };
    if !stamp.is_some_and(|stamp| matches_stamp(&placed, stamp)) {
        return Ok(StepOutcome::Skipped);
    }
    match ops.remove_file(target) {
        Ok(()) => Ok(StepOutcome::Done),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(StepOutcome::Skipped),
        Err(error) => Err(error),
    }
}

fn source_gone(
    job: &Journaled<'_>,
    source: &Location,
    held: Option<&HeldFile>,
    ops: &dyn ProviderOps,
) -> Result<bool, FsError> {
    let removed_by_job = (0_u32..).zip(&job.plan.steps).any(|(seq, step)| {
        matches!(step, Step::RemoveFile { location } if location.same_place(source))
            && job.done.contains(&seq)
    });
    if removed_by_job {
        return Ok(true);
    }
    gone_or_folder_deleted(ops, source, held.and_then(|file| file.volume))
}

fn settle(
    restored: Restored,
    held: &HeldFile,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    Ok(match restored {
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
