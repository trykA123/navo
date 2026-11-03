use navo_core::retention::RetentionPolicy;
use navo_core::{Location, OpKind, OperationPlan};
use navo_fs::ProviderOps;

use super::types::{Ended, KeepOld, LogRefusal, PendingOp, RetainDeletes};
use crate::browser::{Browser, RenameReport};

pub(super) fn delete_retention(
    ops: &dyn ProviderOps,
    plan: &OperationPlan,
    sources: &[Location],
    retain_deletes: Option<&RetainDeletes>,
) -> (Option<RetentionPolicy>, bool) {
    if plan.kind != OpKind::Delete {
        return (None, false);
    }
    let Some(policy) = retain_deletes.and_then(|active| active()) else {
        return (None, false);
    };
    match sources.first() {
        Some(first) if navo_retention::can_retain(ops, first) => (Some(policy), false),
        _ => (None, true),
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
            cannot_retain: false,
            keep_old: KeepOld::Skip,
            structure: None,
            ..pending
        });
        true
    }
}
