use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Resolution};

use super::{StepOutcome, TEMP_SUFFIX};
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

pub(super) fn copy(
    copier: &Copier<'_>,
    step: &CopyStep<'_>,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<StepOutcome, FsError> {
    let exists = copier.ops.metadata(step.target).is_ok();
    if exists && !overwrites(copier.plan, step.target) {
        return Ok(StepOutcome::Skipped);
    }
    let temp = temp_of(step.target)?;
    remove_if_present(copier.ops, &temp)?;
    let placement = Placement {
        step,
        temp: &temp,
        exists,
    };
    let result = write_and_place(copier, &placement, on_bytes);
    if result.is_err() {
        let _ = remove_if_present(copier.ops, &temp);
    }
    result.map(|()| StepOutcome::Done)
}

struct Placement<'a> {
    step: &'a CopyStep<'a>,
    temp: &'a Location,
    exists: bool,
}

fn write_and_place(
    copier: &Copier<'_>,
    placement: &Placement<'_>,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), FsError> {
    let Placement { step, temp, exists } = *placement;
    let Copier { ops, cancel, .. } = *copier;
    let written = ops.copy_file(step.source, temp, on_bytes, cancel)?;
    if cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, step.target));
    }
    let on_disk = ops.metadata(temp)?.size;
    if written != step.size || on_disk != written {
        return Err(failure(FsErrorKind::Unknown, step.target));
    }
    if exists {
        ops.replace(temp, step.target)
    } else {
        ops.rename(temp, step.target)
    }
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

pub(super) fn failure(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

fn invalid(location: &Location) -> FsError {
    failure(FsErrorKind::NameInvalid, location)
}
