mod deletion;
mod inventory;
mod policy;
mod temporary;
mod validation;
mod writing;

use serde::{Deserialize, Serialize};

use crate::{FileIdentity, Location, ProviderPath};

pub use deletion::{
    RETAINED_DELETE_KIND, RetainedDeletePlan, delete_exceeds_capacity, delete_exceeds_limit,
};
pub use inventory::RetentionSummary;
pub use policy::{
    PurgeChoice, PurgePlan, PurgeReason, PurgeRequest, RetentionPolicy, VolumeSpace, select_purge,
    select_purge_inventory,
};
pub use temporary::{CopyTemporary, copy_temporary};
pub use writing::{COPY_RETAINED_KIND, MOVE_RETAINED_KIND, RetainedWritePlan, is_retained_write};

pub const CONTAINER_PREFIX: &str = crate::brand::RETAINED_PREFIX;
pub const ADOPTED_CONTAINER_PREFIX: &str = crate::brand::HELD_PREFIX;
pub const VAULT_RECORD_VERSION: u32 = 3;
pub const MAX_NODES: usize = 100_000;
pub const MAX_RECORD_BYTES: usize = 32 * 1024 * 1024;
pub const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetentionError {
    #[error("invalid retention data: {0}")]
    Invalid(&'static str),
    #[error("delete exceeds the retained node limit")]
    DeleteTooLarge,
    #[error("retention size or time exceeds its supported range")]
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RetentionId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedKind {
    File,
    Folder,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedNode {
    pub path: Vec<String>,
    pub kind: RetainedKind,
    pub identity: FileIdentity,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

impl RetainedNode {
    pub fn location(&self, root: &Location) -> Result<Location, RetentionError> {
        self.path.iter().try_fold(root.clone(), |parent, part| {
            parent
                .join(&ProviderPath::from_text(part))
                .map_err(|_| RetentionError::Invalid("invalid relative node path"))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionRecord {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_name: Option<String>,
    pub source: Location,
    pub container: Location,
    pub parent_identity: FileIdentity,
    pub container_identity: Option<FileIdentity>,
    pub nodes: Vec<RetainedNode>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl RetentionRecord {
    #[must_use]
    pub fn anchor(&self) -> Option<Location> {
        if self.version == VAULT_RECORD_VERSION {
            self.container.parent()
        } else {
            self.source.parent()
        }
    }

    pub fn held(&self) -> Result<Location, RetentionError> {
        self.container
            .join(&ProviderPath::from_text(
                self.payload_name.as_deref().unwrap_or("payload"),
            ))
            .map_err(|_| RetentionError::Invalid("invalid retention container"))
    }

    pub fn bytes(&self) -> Result<u64, RetentionError> {
        self.nodes.iter().try_fold(0_u64, |total, node| {
            total.checked_add(node.size).ok_or(RetentionError::Overflow)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverwriteProof {
    pub parent_identity: FileIdentity,
    pub container_identity: FileIdentity,
    pub mount_identity: u64,
    pub node: RetainedNode,
    pub replacement_identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvedOverwrite {
    pub original: Location,
    pub held: Location,
    pub placed_size: Option<u64>,
    pub placed_modified_ms: Option<i64>,
    pub volume: Option<u64>,
    pub proof: OverwriteProof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainedState {
    Preparing,
    Ready,
    Restoring,
    Restored,
    Purging,
    Purged,
    Aborted,
    Conflict,
}

impl RetainedState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Ready => "ready",
            Self::Restoring => "restoring",
            Self::Restored => "restored",
            Self::Purging => "purging",
            Self::Purged => "purged",
            Self::Aborted => "aborted",
            Self::Conflict => "conflict",
        }
    }

    pub fn parse(text: &str) -> Result<Self, RetentionError> {
        match text {
            "preparing" => Ok(Self::Preparing),
            "ready" => Ok(Self::Ready),
            "restoring" => Ok(Self::Restoring),
            "restored" => Ok(Self::Restored),
            "purging" => Ok(Self::Purging),
            "purged" => Ok(Self::Purged),
            "aborted" => Ok(Self::Aborted),
            "conflict" => Ok(Self::Conflict),
            _ => Err(RetentionError::Invalid("unknown retention state")),
        }
    }

    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Restored | Self::Purged | Self::Aborted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreTarget {
    pub location: Location,
    pub parent_identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionEntry {
    pub id: RetentionId,
    pub job_id: Option<i64>,
    pub pinned: bool,
    pub state: RetainedState,
    pub record: RetentionRecord,
    pub target: Option<RestoreTarget>,
}

#[cfg(test)]
mod tests;
