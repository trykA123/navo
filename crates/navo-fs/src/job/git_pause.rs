//! Job-wide Git pauses: choosing roots, and releasing the pause while the user holds the job.

use std::cell::RefCell;

use navo_core::{FsError, Location, OpKind, OperationPlan, Step};

use super::hold::hold;
use super::{JobContext, JobOutcome, Progress};
use crate::GitPauseGuard;

/// Releases the job-wide pause for a user pause, then takes it back before the next step.
#[expect(clippy::too_many_arguments, reason = "the walk state has five parts")]
pub(super) fn hold_paused(
    plan: &OperationPlan,
    seq: u32,
    ctx: &mut JobContext<'_>,
    pauses: &RefCell<Option<Vec<GitPauseGuard>>>,
    progress: &mut Progress,
) -> Result<(), JobOutcome> {
    if ctx.cancel.is_paused() {
        pauses.replace(None);
    }
    hold(ctx.cancel, progress, ctx.progress);
    if ctx.cancel.is_cancelled() {
        return Err(JobOutcome::Cancelled);
    }
    if pauses.borrow().is_none() {
        pauses.replace(Some(pause_or_fail(plan, seq, ctx)?));
    }
    Ok(())
}

/// Takes the job-wide pause, or the failure at `seq`.
pub(super) fn pause_or_fail(
    plan: &OperationPlan,
    seq: u32,
    ctx: &JobContext<'_>,
) -> Result<Vec<GitPauseGuard>, JobOutcome> {
    pause_job(plan, ctx).map_err(|error| JobOutcome::Failed { seq, error })
}

/// Pauses Git once per chosen top-level root for a delete, or a copy-then-delete move.
pub(super) fn pause_job(
    plan: &OperationPlan,
    ctx: &JobContext<'_>,
) -> Result<Vec<GitPauseGuard>, FsError> {
    if !needs_job_pause(plan) {
        return Ok(Vec::new());
    }
    let mut guards = Vec::new();
    for root in pause_roots(plan) {
        if let Some(guard) = ctx.ops.pause_git(&root)? {
            guards.push(guard);
        }
    }
    Ok(guards)
}

/// Whether the plan deletes files after copying or outright.
fn needs_job_pause(plan: &OperationPlan) -> bool {
    match plan.kind {
        OpKind::Delete => !plan.steps.is_empty(),
        OpKind::Move => plan
            .steps
            .iter()
            .any(|step| matches!(step, Step::RemoveFile { .. } | Step::RemoveFolder { .. })),
        _ => false,
    }
}

/// The distinct top-level roots the plan removes or copies from.
pub(super) fn pause_roots(plan: &OperationPlan) -> Vec<Location> {
    let mut candidates: Vec<(Vec<String>, Location)> = plan
        .steps
        .iter()
        .filter_map(|step| {
            let location = match step {
                Step::RemoveFile { location } | Step::RemoveFolder { location } => location,
                Step::CopyFile { source, .. } if plan.kind == OpKind::Move => source,
                _ => return None,
            };
            Some((path_key(location), location.clone()))
        })
        .collect();
    candidates.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut roots: Vec<Location> = Vec::new();
    for (_, location) in candidates {
        let covered = roots
            .last()
            .is_some_and(|last| last.same_place(&location) || below(&location, last));
        if !covered {
            roots.push(location);
        }
    }
    roots
}

/// The path components of a location, folded on Windows for component-wise sorting.
fn path_key(location: &Location) -> Vec<String> {
    let mut parts = Vec::new();
    let mut at = Some(location.clone());
    while let Some(current) = at {
        if let Some(name) = current.name() {
            let text = name.to_text();
            parts.push(if cfg!(windows) {
                text.to_lowercase()
            } else {
                text
            });
        }
        at = current.parent();
    }
    parts.reverse();
    parts
}

/// Whether `child` lies inside `ancestor`.
fn below(child: &Location, ancestor: &Location) -> bool {
    std::iter::successors(child.parent(), Location::parent)
        .any(|parent| parent.same_place(ancestor))
}
