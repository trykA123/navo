use navo_core::retention::RetentionPolicy;
use navo_core::{Location, OpKind, OperationPlan};
use navo_fs::ProviderOps;

use navo_retention::KeepCheck;

use super::types::{Ended, KeepOld, LogRefusal, NoKeep, PendingOp, RetainDeletes};
use crate::browser::{Browser, RenameReport};

/// The policy a delete keeps copies under, or why it keeps none. Every source folder is checked:
/// one full drive (AD14), one unsupported drive or a failed check plans the whole delete without.
pub(super) fn delete_retention(
    ops: &dyn ProviderOps,
    plan: &OperationPlan,
    sources: &[Location],
    retain_deletes: Option<&RetainDeletes>,
) -> (Option<RetentionPolicy>, Option<NoKeep>) {
    if plan.kind != OpKind::Delete {
        return (None, None);
    }
    let Some(policy) = retain_deletes.and_then(|active| active()) else {
        return (None, None);
    };
    if sources.is_empty() {
        return (None, Some(NoKeep::Drive));
    }
    match navo_retention::check_keep_all(ops, sources, policy) {
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
