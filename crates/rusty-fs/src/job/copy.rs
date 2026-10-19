use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Resolution};

use super::hold::hold;
use super::{JobContext, Progress, StepOutcome, TEMP_SUFFIX};
use crate::ProviderOps;

pub(super) struct CopyStep<'a> {
    pub source: &'a Location,
    pub target: &'a Location,
    pub size: u64,
}

pub(super) fn temp_of(target: &Location) -> Result<Location, FsError> {
    let name = target.name().ok_or_else(|| invalid(target))?;
    let parent = target.parent().ok_or_else(|| invalid(target))?;
    let temp = rusty_core::ProviderPath::from_text(&format!("{}{TEMP_SUFFIX}", name.to_text()));
    parent.join(&temp).map_err(|_| invalid(target))
}

pub(super) fn copy(
    plan: &OperationPlan,
    step: &CopyStep<'_>,
    ctx: &mut JobContext<'_>,
    progress: &mut Progress,
) -> Result<StepOutcome, FsError> {
    let exists = ctx.ops.metadata(step.target).is_ok();
    if exists && !overwrites(plan, step.target) {
        return Ok(StepOutcome::Skipped);
    }
    let temp = temp_of(step.target)?;
    remove_if_present(ctx.ops, &temp)?;
    let placement = Placement {
        step,
        temp: &temp,
        exists,
    };
    let result = write_and_place(&placement, ctx, progress);
    if result.is_err() {
        let _ = remove_if_present(ctx.ops, &temp);
    }
    result.map(|()| StepOutcome::Done)
}

struct Placement<'a> {
    step: &'a CopyStep<'a>,
    temp: &'a Location,
    exists: bool,
}

fn write_and_place(
    placement: &Placement<'_>,
    ctx: &mut JobContext<'_>,
    progress: &mut Progress,
) -> Result<(), FsError> {
    let Placement { step, temp, exists } = *placement;
    let base = progress.bytes_done;
    let (cancel, report) = (ctx.cancel, &mut *ctx.progress);
    let mut on_bytes = |bytes: u64| {
        progress.bytes_done = base + bytes;
        report(progress);
        hold(cancel, progress, report);
    };
    let written = ctx
        .ops
        .copy_file(step.source, temp, &mut on_bytes, cancel)?;
    if cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, step.target));
    }
    let on_disk = ctx.ops.metadata(temp)?.size;
    if written != step.size || on_disk != written {
        return Err(failure(FsErrorKind::Unknown, step.target));
    }
    if exists {
        ctx.ops.remove_file(step.target)?;
    }
    ctx.ops.rename(temp, step.target)
}

fn overwrites(plan: &OperationPlan, target: &Location) -> bool {
    plan.conflicts
        .iter()
        .any(|c| c.resolution == Resolution::Overwrite && c.target.same_place(target))
}

pub(super) fn remove_if_present(
    ops: &dyn ProviderOps,
    location: &Location,
) -> Result<bool, FsError> {
    if ops.metadata(location).is_err() {
        return Ok(false);
    }
    ops.remove_file(location).map(|()| true)
}

fn failure(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

fn invalid(location: &Location) -> FsError {
    failure(FsErrorKind::NameInvalid, location)
}
