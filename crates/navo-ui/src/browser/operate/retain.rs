//! Retention checks for planned jobs: whether a delete keeps old versions, whether the drive a
//! delete or an overwrite works on can keep them, and the "run without old versions" question.

use navo_core::retention::RetentionPolicy;
use navo_core::{OpKind, OperationPlan};
use navo_fs::ProviderOps;

use navo_retention::KeepCheck;

use super::types::{Ended, KeepOld, LogRefusal, NoKeep, OpRequest, PendingOp, RetainDeletes};
use crate::browser::{Browser, RenameReport};

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
            ..pending
        });
        true
    }
}
