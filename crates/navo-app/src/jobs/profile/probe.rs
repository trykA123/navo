use std::time::Duration;

use navo_core::retention::RetentionPolicy;
use navo_core::{FsError, Location, OpKind, OperationPlan, Resolution, Step};
use navo_fs::ProviderOps;
use navo_retention::KeepCheck;
use navo_store::RetentionSettings;
use navo_ui::browser::LogRefusal;

use super::Drive;

/// The settings a job runs with on `drive`: retention is off when the drive cannot keep, or when
/// a delete touches a drive below "Stop below free space" (AD14). Unknown support refuses the job.
pub(super) fn drive_keeps(
    settings: RetentionSettings,
    drive: Option<&Drive>,
    plan: &OperationPlan,
) -> Result<RetentionSettings, LogRefusal> {
    if settings.maintenance_policy().is_none() {
        return Ok(settings);
    }
    let keeps = match (drive, settings.delete_policy()) {
        (Some(Drive::Probed(ops)), Some(policy)) if plan.kind == OpKind::Delete => {
            delete_keeps(ops.as_ref(), plan, policy)?
        }
        _ => drive_retains(drive, plan)?,
    };
    Ok(if keeps {
        settings
    } else {
        RetentionSettings::off()
    })
}

/// A delete keeps copies only when every removed root's folder can keep one (AD14): a full or
/// unsupported drive says no, so the delete runs as the user confirmed it. Vanished roots hold
/// nothing; a check that fails three times refuses the job.
fn delete_keeps(
    ops: &dyn ProviderOps,
    plan: &OperationPlan,
    policy: RetentionPolicy,
) -> Result<bool, LogRefusal> {
    let roots = removed_roots(plan);
    if roots.is_empty() {
        return Ok(false);
    }
    match retried(|| navo_retention::check_keep_all(ops, roots.iter().copied(), policy))? {
        KeepCheck::Keep => Ok(true),
        check => {
            tracing::info!(?check, "delete keeps no copy on this drive");
            Ok(false)
        }
    }
}

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
        OpKind::Delete => match removed_roots(plan) {
            roots if roots.is_empty() => Ok(false),
            roots => first_answer(ops.as_ref(), roots.into_iter().take(MAX_PLAN_PROBES)),
        },
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

/// The selected items a delete removes: a plan lists every descendant too, so only paths with no
/// removed ancestor stay, the same roots the retained delete keeps.
fn removed_roots(plan: &OperationPlan) -> Vec<&Location> {
    let roots = navo_core::retention::delete_roots(&plan.steps);
    roots.into_iter().map(|(_, root)| root).collect()
}

/// Pauses between retention probes: three tries, well under half a second in total.
const PROBE_PAUSES: [Duration; 2] = [Duration::from_millis(100), Duration::from_millis(250)];

/// `None` when the probed path does not exist.
fn check_support(ops: &dyn ProviderOps, at: &Location) -> Result<Option<bool>, LogRefusal> {
    retried(|| navo_retention::check_target_support(ops, at))
}

/// Runs a retention probe up to three times; a probe that still fails refuses the job.
fn retried<T>(probe: impl Fn() -> Result<T, FsError>) -> Result<T, LogRefusal> {
    let mut checked = probe();
    for pause in PROBE_PAUSES {
        let Err(error) = &checked else {
            break;
        };
        tracing::debug!(error_kind = ?error.kind, "retention probe failed; retrying");
        std::thread::sleep(pause);
        checked = probe();
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
