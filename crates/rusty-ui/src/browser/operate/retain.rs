use rusty_core::retention::RetentionPolicy;
use rusty_core::{Location, OpKind, OperationPlan};
use rusty_fs::ProviderOps;

use super::types::RetainDeletes;

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
        Some(first) if rusty_retention::can_retain(ops, first) => (Some(policy), false),
        _ => (None, true),
    }
}
