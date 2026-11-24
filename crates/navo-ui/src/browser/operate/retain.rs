//! Retention checks for planned jobs: whether a delete keeps old versions, whether the drive a
//! delete or an overwrite works on can keep them, and the "run without old versions" question.

use navo_core::retention::RetentionPolicy;
use navo_core::{OpKind, OperationPlan};
use navo_fs::ProviderOps;

use navo_retention::KeepCheck;

use super::queue::{self, SlowVolume};
use super::types::{
    ClassOf, Ended, KeepOld, LogRefusal, NoKeep, OpRequest, PendingOp, PlannedOp, RetainDeletes,
    RetentionChecks, Structure,
};
use super::{clashes, verify};
use crate::browser::{Browser, RenameReport};

/// Builds what the confirm dialog shows for a finished plan: its clashes, the retention it runs
/// with, whether Undo can restore an overwrite, the slow volumes it touches and the target's
/// drive class. `facts` is (retention checks, slow volume check, drive classifier).
pub(super) fn planned_op(
    ops: &dyn ProviderOps,
    plan: OperationPlan,
    request: &OpRequest,
    facts: (&RetentionChecks, &SlowVolume, ClassOf),
) -> PlannedOp {
    let (checks, slow, class_of) = facts;
    let (retain_deletes, cannot_retain) =
        drive_retention(ops, &plan, request, checks.deletes.as_ref());
    let places = request.sources.iter().chain(request.target.iter());
    PlannedOp {
        clashes: clashes::clashes(ops, &plan),
        plan,
        retain_deletes,
        cannot_retain,
        slow_volumes: queue::slow_volumes(places, slow),
        target_class: verify::target_class(class_of, request.target.as_ref()),
        undoes_overwrites: checks.overwrites.as_ref().is_some_and(|keeps| keeps()),
    }
}

/// The pending op the confirm dialog shows for `planned`, the plan built for `request`.
pub(super) fn pending_op(request: OpRequest, planned: PlannedOp) -> PendingOp {
    let replan = super::keep::replan_of(&request);
    PendingOp {
        id: request.id,
        sources: request.sources,
        target: request.target,
        plan: planned.plan,
        retain_deletes: planned.retain_deletes,
        cannot_retain: planned.cannot_retain,
        keep_old: KeepOld::AsSet,
        source_tab: request.source_tab,
        undo: None,
        slow_volumes: Some(planned.slow_volumes),
        structure: request.structure.map(|s| Structure {
            planning: false,
            ..s
        }),
        stream: request.stream,
        replan,
        clashes: planned.clashes,
        undoes_overwrites: planned.undoes_overwrites,
    }
}

/// The retention a planned job runs with, and why the dialog warns that its drive keeps nothing.
/// A delete checks every source folder (AD14: one full or unsupported drive, or a failed check,
/// plans it without); a copy or move that meets names already there probes the target folder
/// (AD15), and a failed probe warns. Both need retention on.
pub(super) fn drive_retention(
    ops: &dyn ProviderOps,
    plan: &OperationPlan,
    request: &OpRequest,
    retain_deletes: Option<&RetainDeletes>,
) -> (Option<RetentionPolicy>, Option<NoKeep>) {
    let replaces = matches!(plan.kind, OpKind::Copy | OpKind::Move) && !plan.conflicts.is_empty();
    if plan.kind != OpKind::Delete && !replaces {
        return (None, None);
    }
    let Some(policy) = retain_deletes.and_then(|active| active()) else {
        return (None, None);
    };
    if replaces {
        let kept = request
            .target
            .as_ref()
            .is_some_and(|target| navo_retention::can_retain(ops, target));
        return (None, (!kept).then_some(NoKeep::Drive));
    }
    if request.sources.is_empty() {
        return (None, Some(NoKeep::Drive));
    }
    match navo_retention::check_keep_all(ops, &request.sources, policy) {
        Ok(KeepCheck::Keep) => (Some(policy), None),
        Ok(KeepCheck::DriveFull) => (None, Some(NoKeep::LowSpace)),
        Ok(KeepCheck::Unsupported) => (None, Some(NoKeep::Drive)),
        Err(error) => {
            tracing::warn!(error_kind = ?error.kind, "keep check failed; delete planned without a copy");
            (None, Some(NoKeep::Drive))
        }
    }
}

impl Browser {
    /// A job refused for unknown retention never ran; ask once whether to run it without.
    pub(super) fn ask_without_retention(&mut self, id: u64, pending: PendingOp) -> bool {
        let can_ask = pending.keep_old == KeepOld::AsSet
            && self.pending.is_none()
            && self.operations.log_without_retention.is_some();
        if !can_ask {
            let ended = Ended {
                progress: None,
                outcome: Err(LogRefusal::RetentionUnknown.message()),
                report: RenameReport::default(),
            };
            return self.finish_job(id, ended);
        }
        self.invalidate_undo();
        self.jobs.retain(|job| job.id != id);
        self.check_left(id);
        self.release_job(id);
        self.next_op += 1;
        self.dialog_opened();
        self.pending = Some(PendingOp {
            id: self.next_op,
            retain_deletes: None,
            cannot_retain: None,
            keep_old: KeepOld::Skip,
            structure: None,
            undoes_overwrites: false,
            ..pending
        });
        true
    }
}
