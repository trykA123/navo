use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Step};

use super::copy::{CopyStep, copy};
use super::{JobContext, JobOutcome, Progress, StepOutcome};

pub(super) fn current_of(step: &Step) -> Location {
    match step {
        Step::MakeFolder { target } => target.clone(),
        Step::CopyFile { source, .. } | Step::Rename { source, .. } => source.clone(),
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => location.clone(),
    }
}

fn counted(step: &Step) -> (u64, u64) {
    match step {
        Step::CopyFile { size, .. } => (*size, 1),
        Step::RemoveFile { .. } => (0, 1),
        _ => (0, 0),
    }
}

pub(super) fn run(plan: &OperationPlan, done: &[u32], ctx: &mut JobContext<'_>) -> JobOutcome {
    let mut progress = Progress {
        bytes_done: 0,
        bytes_total: plan.totals.bytes,
        files_done: 0,
        files_total: plan.totals.files,
        current: None,
    };
    let mut kept = Vec::new();
    for (seq, step) in (0_u32..).zip(&plan.steps) {
        if done.contains(&seq) {
            advance(&mut progress, step);
            continue;
        }
        if ctx.cancel.is_cancelled() {
            return JobOutcome::Cancelled;
        }
        progress.current = Some(current_of(step));
        let start = progress.bytes_done;
        let result = if holds_kept(step, &kept) {
            Ok(StepOutcome::Skipped)
        } else {
            apply(plan, step, ctx, &mut progress)
        };
        match result {
            Ok(outcome) => {
                note_kept(step, outcome, &mut kept);
                if let Err(error) = ctx.log.step(seq, outcome) {
                    return JobOutcome::Failed { seq, error };
                }
                progress.bytes_done = start;
                advance(&mut progress, step);
                (ctx.progress)(&progress);
            }
            Err(error) if error.kind == FsErrorKind::Cancelled || ctx.cancel.is_cancelled() => {
                return JobOutcome::Cancelled;
            }
            Err(error) => {
                let _ = ctx.log.step(seq, StepOutcome::Failed);
                return JobOutcome::Failed { seq, error };
            }
        }
    }
    JobOutcome::Done
}

fn note_kept(step: &Step, outcome: StepOutcome, kept: &mut Vec<Location>) {
    if let Step::CopyFile { source, .. } = step
        && outcome != StepOutcome::Done
    {
        kept.push(source.clone());
    }
}

fn holds_kept(step: &Step, kept: &[Location]) -> bool {
    let location = match step {
        Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => location,
        _ => return false,
    };
    kept.iter().any(|source| {
        std::iter::successors(Some(source.clone()), Location::parent)
            .any(|ancestor| ancestor.same_place(location))
    })
}

fn advance(progress: &mut Progress, step: &Step) {
    let (bytes, files) = counted(step);
    progress.bytes_done += bytes;
    progress.files_done += files;
}

fn apply(
    plan: &OperationPlan,
    step: &Step,
    ctx: &mut JobContext<'_>,
    progress: &mut Progress,
) -> Result<StepOutcome, FsError> {
    let ops = ctx.ops;
    let done = |result: Result<(), FsError>| result.map(|()| StepOutcome::Done);
    match step {
        Step::MakeFolder { target } => make_folder(ctx, target),
        Step::CopyFile {
            source,
            target,
            size,
        } => {
            let step = CopyStep {
                source,
                target,
                size: *size,
            };
            copy(plan, &step, ctx, progress)
        }
        Step::Rename { source, target } => done(ops.rename(source, target)),
        Step::Recycle { location } => done(ops.recycle(location)),
        Step::RemoveFile { location } => done(ops.remove_file(location)),
        Step::RemoveLink { location } => done(ops.remove_link(location)),
        Step::RemoveFolder { location } => done(ops.remove_empty_folder(location)),
    }
}

fn make_folder(ctx: &mut JobContext<'_>, target: &Location) -> Result<StepOutcome, FsError> {
    let existing = ctx.ops.metadata(target);
    if existing.is_ok_and(|entry| entry.kind == crate::EntryKind::Folder) {
        return Ok(StepOutcome::Skipped);
    }
    ctx.ops.create_folder(target).map(|()| StepOutcome::Done)
}
