use serde::{Deserialize, Serialize};

use super::{MAX_NODES, RetentionError, RetentionPolicy};
use crate::{OpKind, OperationPlan, Step};

pub const COPY_RETAINED_KIND: &str = "copy-retained-v1";
pub const MOVE_RETAINED_KIND: &str = "move-retained-v1";

pub fn is_retained_write(kind: &str) -> bool {
    matches!(kind, COPY_RETAINED_KIND | MOVE_RETAINED_KIND)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WriteData", into = "WriteData")]
pub struct RetainedWritePlan {
    plan: OperationPlan,
    policy: RetentionPolicy,
    temporary_namespace: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct WriteData {
    #[serde(flatten)]
    plan: OperationPlan,
    overwrite_retention_version: u32,
    retention: RetentionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temporary_namespace: Option<String>,
}

impl RetainedWritePlan {
    pub fn new(plan: OperationPlan, policy: RetentionPolicy) -> Result<Self, RetentionError> {
        if !matches!(plan.kind, OpKind::Copy | OpKind::Move) || plan.steps.len() > MAX_NODES {
            return Err(RetentionError::Invalid("invalid retained write plan"));
        }
        for step in &plan.steps {
            let valid = match step {
                Step::MakeFolder { .. } | Step::CopyFile { .. } => true,
                Step::Rename { .. }
                | Step::RemoveFile { .. }
                | Step::RemoveLink { .. }
                | Step::RemoveFolder { .. } => plan.kind == OpKind::Move,
                _ => false,
            };
            if !valid {
                return Err(RetentionError::Invalid("invalid retained write step"));
            }
        }
        Ok(Self {
            plan,
            policy,
            temporary_namespace: None,
        })
    }

    pub fn with_temporary_namespace(mut self, namespace: String) -> Result<Self, RetentionError> {
        super::temporary::validate_namespace(&namespace)?;
        self.temporary_namespace = Some(namespace);
        Ok(self)
    }

    pub fn temporary_namespace(&self) -> Option<&str> {
        self.temporary_namespace.as_deref()
    }

    pub fn plan(&self) -> &OperationPlan {
        &self.plan
    }
    pub fn policy(&self) -> RetentionPolicy {
        self.policy
    }
    pub fn journal_kind(&self) -> &'static str {
        if self.plan.kind == OpKind::Copy {
            COPY_RETAINED_KIND
        } else {
            MOVE_RETAINED_KIND
        }
    }
}

impl TryFrom<WriteData> for RetainedWritePlan {
    type Error = RetentionError;

    fn try_from(data: WriteData) -> Result<Self, Self::Error> {
        let plan = Self::new(data.plan, data.retention)?;
        match (data.overwrite_retention_version, data.temporary_namespace) {
            (1, None) => Ok(plan),
            (2, Some(namespace)) => plan.with_temporary_namespace(namespace),
            _ => Err(RetentionError::Invalid(
                "unsupported overwrite retention version",
            )),
        }
    }
}

impl From<RetainedWritePlan> for WriteData {
    fn from(plan: RetainedWritePlan) -> Self {
        Self {
            plan: plan.plan,
            overwrite_retention_version: if plan.temporary_namespace.is_some() {
                2
            } else {
                1
            },
            retention: plan.policy,
            temporary_namespace: plan.temporary_namespace,
        }
    }
}

#[cfg(test)]
mod tests;
