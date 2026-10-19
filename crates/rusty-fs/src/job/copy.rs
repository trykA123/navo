use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Resolution};

use super::aside::Aside;
use super::{FileStamp, JobContext, PlacedFile, StepOutcome, TEMP_SUFFIX};
use crate::{CancelToken, ProviderOps};

pub(super) struct Copier<'a> {
    pub plan: &'a OperationPlan,
    pub ops: &'a dyn ProviderOps,
    pub cancel: &'a CancelToken,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Copied {
    Skipped,
    Placed(FileStamp),
    Ready,
}

pub(super) fn copy(
    copier: &Copier<'_>,
    step: &CopyStep<'_>,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<Copied, FsError> {
    let exists = copier.ops.metadata(step.target).is_ok();
    if exists && !overwrites(copier.plan, step.target) {
        return Ok(Copied::Skipped);
    }
    let temp = temp_of(step.target)?;
    remove_if_present(copier.ops, &temp)?;
    let result = write(copier, step, &temp, on_bytes).and_then(|stamp| {
        if exists {
            Ok(Copied::Ready)
        } else {
            copier
                .ops
                .rename(&temp, step.target)
                .map(|()| Copied::Placed(stamp))
        }
    });
    if result.is_err() {
        let _ = remove_if_present(copier.ops, &temp);
    }
    result
}

pub(super) fn place(
    aside: &Aside,
    ctx: &mut JobContext<'_>,
    copied: Copied,
    target: &Location,
) -> Result<StepOutcome, FsError> {
    match copied {
        Copied::Skipped => Ok(StepOutcome::Skipped),
        Copied::Placed(stamp) => journal_placed(ctx, target, stamp).map(|()| StepOutcome::Done),
        Copied::Ready => {
            let placed = aside.place_over(ctx, target);
            if placed.is_err() {
                let _ = temp_of(target).map(|temp| remove_if_present(ctx.ops, &temp));
            }
            placed.map(|()| StepOutcome::Done)
        }
    }
}

pub(super) fn journal_placed(
    ctx: &mut JobContext<'_>,
    target: &Location,
    stamp: FileStamp,
) -> Result<(), FsError> {
    ctx.log.journal_placed(&PlacedFile {
        target: target.clone(),
        stamp,
    })
}

fn write(
    copier: &Copier<'_>,
    step: &CopyStep<'_>,
    temp: &Location,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<FileStamp, FsError> {
    let Copier { ops, cancel, .. } = *copier;
    let written = ops.copy_file(step.source, temp, on_bytes, cancel)?;
    if cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, step.target));
    }
    let on_disk = ops.metadata(temp)?;
    if written != step.size || on_disk.size != written {
        return Err(failure(FsErrorKind::Unknown, step.target));
    }
    Ok(FileStamp {
        size: on_disk.size,
        modified_ms: on_disk.modified_ms,
    })
}

pub(super) fn overwrites(plan: &OperationPlan, target: &Location) -> bool {
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

pub(super) fn failure(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

fn invalid(location: &Location) -> FsError {
    failure(FsErrorKind::NameInvalid, location)
}
