use std::time::Duration;

use navo_core::{Location, OpKind, OperationPlan, Resolution, Step};
use navo_fs::ProviderOps;
use navo_ui::browser::LogRefusal;

use super::Drive;

pub(super) fn drive_retains(
    drive: Option<&Drive>,
    plan: &OperationPlan,
) -> Result<bool, LogRefusal> {
    let ops = match drive {
        Some(Drive::Probed(ops)) => ops,
        #[cfg(test)]
        Some(Drive::Unprobed(_)) => return Ok(true),
        Some(Drive::Declined) | None => return Ok(false),
    };
    match plan.kind {
        OpKind::Delete if removed_roots(plan).next().is_none() => Ok(false),
        OpKind::Delete => first_answer(ops.as_ref(), removed_roots(plan)),
        OpKind::Copy | OpKind::Move | OpKind::Save => {
            first_answer(ops.as_ref(), overwritten_targets(plan))
        }
        _ => Ok(true),
    }
}

/// At most this many planned paths are probed beyond the confirmed overwrite conflicts.
const MAX_PLAN_PROBES: usize = 16;

/// The first existing path decides; a vanished path holds nothing, and none left keeps retention.
fn first_answer<'a>(
    ops: &dyn ProviderOps,
    paths: impl Iterator<Item = &'a Location>,
) -> Result<bool, LogRefusal> {
    for at in paths {
        if let Some(supported) = check_support(ops, at)? {
            return Ok(supported);
        }
    }
    Ok(true)
}

fn removed_roots(plan: &OperationPlan) -> impl Iterator<Item = &Location> {
    let removed = plan.steps.iter().filter_map(|step| match step {
        Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => Some(location),
        _ => None,
    });
    removed.take(MAX_PLAN_PROBES)
}

/// Pauses between retention probes: three tries, well under half a second in total.
const PROBE_PAUSES: [Duration; 2] = [Duration::from_millis(100), Duration::from_millis(250)];

/// `None` when the probed path does not exist.
fn check_support(ops: &dyn ProviderOps, at: &Location) -> Result<Option<bool>, LogRefusal> {
    let mut checked = navo_retention::check_target_support(ops, at);
    for pause in PROBE_PAUSES {
        let Err(error) = &checked else {
            break;
        };
        tracing::debug!(error_kind = ?error.kind, "retention probe failed; retrying");
        std::thread::sleep(pause);
        checked = navo_retention::check_target_support(ops, at);
    }
    checked.map_err(|error| {
        tracing::warn!(error_kind = ?error.kind, "retention support unknown; job refused");
        LogRefusal::RetentionUnknown
    })
}

pub(super) fn retains(kind: OpKind) -> bool {
    matches!(
        kind,
        OpKind::Copy | OpKind::Move | OpKind::Delete | OpKind::Save
    )
}

pub(super) fn overwritten_targets(plan: &OperationPlan) -> impl Iterator<Item = &Location> {
    let conflicts = plan
        .conflicts
        .iter()
        .filter(|conflict| conflict.resolution == Resolution::Overwrite)
        .map(|conflict| &conflict.target);
    let writes = plan.steps.iter().filter_map(|step| match step {
        Step::WriteFile { target, .. } => Some(target),
        _ => None,
    });
    conflicts.chain(writes.take(MAX_PLAN_PROBES))
}
