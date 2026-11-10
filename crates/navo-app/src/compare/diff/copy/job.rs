//! Runs the one-step `Save` plan of a copy through the compare job factory.

use navo_core::{FsErrorKind, OperationPlan};
use navo_fs::{JobContext, JobOutcome, Progress, run_job};
use navo_ui::browser::LoggedOperation;

use super::{CopyContext, CopyRefusal};

/// Log, check, run and finish the plan like a pane job. A stamp mismatch means "changed".
pub(in crate::compare::diff) fn run(
    ctx: &CopyContext<'_>,
    plan: &OperationPlan,
) -> Result<(), CopyRefusal> {
    let mut log = (ctx.factory)(plan).map_err(|refusal| CopyRefusal::Failed(refusal.message()))?;
    let mut progress = |_: &Progress| {};
    let mut waiting = |_: bool| {};
    log.check(logged(ctx, plan, &mut progress, &mut waiting))
        .map_err(CopyRefusal::Failed)?;
    let custom = log.execute(logged(ctx, plan, &mut progress, &mut waiting));
    let outcome = custom.unwrap_or_else(|| {
        run_job(
            plan,
            &[],
            &mut JobContext {
                ops: ctx.ops,
                log: log.as_mut(),
                cancel: ctx.cancel,
                progress: &mut progress,
            },
        )
    });
    let finished = log.finish(&outcome, ctx.ops);
    match outcome {
        JobOutcome::Done => finished.map_err(CopyRefusal::Failed),
        JobOutcome::Failed { error, .. } if error.kind == FsErrorKind::ChangedOnDisk => {
            Err(CopyRefusal::Changed)
        }
        JobOutcome::Failed { error, .. } => Err(CopyRefusal::Failed(error.message())),
        JobOutcome::Cancelled => Err(CopyRefusal::Failed(navo_l10n::tr("compare-reason-stopped"))),
    }
}

/// The operation a log checks and may execute itself; no retention is requested.
fn logged<'a>(
    ctx: &'a CopyContext<'_>,
    plan: &'a OperationPlan,
    progress: &'a mut dyn FnMut(&Progress),
    waiting: &'a mut dyn FnMut(bool),
) -> LoggedOperation<'a> {
    LoggedOperation {
        plan,
        ops: ctx.ops,
        cancel: ctx.cancel,
        progress,
        waiting,
        retain_deletes: None,
    }
}
