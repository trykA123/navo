use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{MAX_NODES, RetentionError, RetentionPolicy, validation};
use crate::{Location, OpKind, OperationPlan, Step};

pub const RETAINED_DELETE_KIND: &str = "delete-retained-v1";

pub fn delete_exceeds_limit(plan: &OperationPlan) -> bool {
    plan.kind == OpKind::Delete && plan.steps.len() > MAX_NODES
}

pub fn delete_exceeds_capacity(plan: &OperationPlan, policy: RetentionPolicy) -> bool {
    delete_exceeds_limit(plan)
        || (plan.kind == OpKind::Delete && plan.totals.bytes > policy.max_volume_bytes())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DeleteData", into = "DeleteData")]
pub struct RetainedDeletePlan {
    plan: OperationPlan,
    policy: RetentionPolicy,
}

#[derive(Serialize, Deserialize)]
struct DeleteData {
    #[serde(flatten)]
    plan: OperationPlan,
    retention_version: u32,
    retention: RetentionPolicy,
}

impl RetainedDeletePlan {
    pub fn new(plan: OperationPlan, policy: RetentionPolicy) -> Result<Self, RetentionError> {
        let validated = Self::validated(plan, policy)?;
        if delete_exceeds_capacity(&validated.plan, policy) {
            return Err(RetentionError::DeleteTooLarge);
        }
        Ok(validated)
    }

    fn validated(plan: OperationPlan, policy: RetentionPolicy) -> Result<Self, RetentionError> {
        if plan.kind != OpKind::Delete || !plan.conflicts.is_empty() {
            return Err(RetentionError::Invalid("invalid retained delete plan"));
        }
        let mut paths = HashSet::new();
        for step in &plan.steps {
            let path = removed(step)?;
            if !validation::is_absolute(path)
                || !validation::clean_path(path)
                || path.parent().is_none()
                || !paths.insert(path)
            {
                return Err(RetentionError::Invalid("invalid or duplicate delete path"));
            }
        }
        if delete_exceeds_limit(&plan) {
            return Err(RetentionError::DeleteTooLarge);
        }
        Ok(Self { plan, policy })
    }

    pub fn plan(&self) -> &OperationPlan {
        &self.plan
    }

    pub fn policy(&self) -> RetentionPolicy {
        self.policy
    }

    pub fn roots(&self) -> Vec<Location> {
        self.root_steps()
            .into_iter()
            .map(|(_, path)| path)
            .collect()
    }

    pub fn root_steps(&self) -> Vec<(u32, Location)> {
        delete_roots(&self.plan.steps)
            .into_iter()
            .map(|(seq, path)| (seq, path.clone()))
            .collect()
    }
}

/// The top-level items a delete removes, with their step numbers: a removed path whose ancestor
/// is also removed is dropped. Steps that remove nothing are passed over.
pub fn delete_roots(steps: &[Step]) -> Vec<(u32, &Location)> {
    let paths: HashSet<_> = steps.iter().filter_map(|step| removed(step).ok()).collect();
    (0_u32..)
        .zip(steps)
        .filter_map(|(seq, step)| {
            let path = removed(step).ok()?;
            let nested = std::iter::successors(path.parent(), Location::parent)
                .any(|parent| paths.contains(&parent));
            (!nested).then_some((seq, path))
        })
        .collect()
}

fn removed(step: &Step) -> Result<&Location, RetentionError> {
    match step {
        Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => Ok(location),
        _ => Err(RetentionError::Invalid(
            "retained delete contains a non-delete step",
        )),
    }
}

impl TryFrom<DeleteData> for RetainedDeletePlan {
    type Error = RetentionError;

    fn try_from(data: DeleteData) -> Result<Self, Self::Error> {
        if data.retention_version != 1 {
            return Err(RetentionError::Invalid(
                "unsupported retained delete version",
            ));
        }
        Self::validated(data.plan, data.retention)
    }
}

impl From<RetainedDeletePlan> for DeleteData {
    fn from(plan: RetainedDeletePlan) -> Self {
        Self {
            plan: plan.plan,
            retention_version: 1,
            retention: plan.policy,
        }
    }
}

#[cfg(test)]
mod tests;
