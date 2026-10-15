use rusty_core::{FsError, OperationPlan, Step};

use super::copy::{remove_if_present, temp_of};
use super::{JobContext, JobOutcome, StepOutcome};
use crate::ProviderOps;

pub(super) fn roll_back(
    plan: &OperationPlan,
    done: &[u32],
    ctx: &mut JobContext<'_>,
) -> JobOutcome {
    let steps: Vec<(u32, &Step)> = (0_u32..).zip(&plan.steps).collect();
    for &(seq, step) in steps.iter().rev() {
        if !done.contains(&seq) {
            continue;
        }
        let outcome = match undo(step, ctx.ops) {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = ctx.log.step(seq, StepOutcome::Failed);
                return JobOutcome::Failed { seq, error };
            }
        };
        if let Err(error) = ctx.log.step(seq, outcome) {
            return JobOutcome::Failed { seq, error };
        }
    }
    JobOutcome::Done
}

fn undo(step: &Step, ops: &dyn ProviderOps) -> Result<StepOutcome, FsError> {
    match step {
        Step::CopyFile { source, target, .. } => {
            if ops.metadata(source).is_err() {
                return Ok(StepOutcome::Skipped);
            }
            ops.remove_file(target).map(|()| StepOutcome::Done)
        }
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
