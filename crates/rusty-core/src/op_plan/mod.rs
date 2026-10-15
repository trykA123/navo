use serde::{Deserialize, Serialize};

use crate::Location;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    Copy,
    Move,
    Recycle,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    File,
    Folder,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictPolicy {
    Ask,
    Skip,
    Rename,
    OverwriteOlder,
    OverwriteAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedItem {
    pub location: Location,
    pub depth: u32,
    pub kind: ItemKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingItem {
    pub location: Location,
    pub kind: ItemKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRequest {
    pub kind: OpKind,
    pub items: Vec<ScannedItem>,
    pub target: Option<Location>,
    pub existing: Vec<ExistingItem>,
    pub policy: ConflictPolicy,
    pub same_volume: bool,
    pub protected: Vec<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    MakeFolder {
        target: Location,
    },
    CopyFile {
        source: Location,
        target: Location,
        size: u64,
    },
    Rename {
        source: Location,
        target: Location,
    },
    Recycle {
        location: Location,
    },
    RemoveFile {
        location: Location,
    },
    RemoveLink {
        location: Location,
    },
    RemoveFolder {
        location: Location,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Skip,
    Rename(Location),
    Overwrite,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub source: Location,
    pub target: Location,
    pub resolution: Resolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    Permanent,
    Protected(Location),
    ManyItems(u64),
    Large(u64),
    LinksSkipped(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationPlan {
    pub kind: OpKind,
    pub steps: Vec<Step>,
    pub totals: Totals,
    pub conflicts: Vec<Conflict>,
    pub risks: Vec<Risk>,
}

impl OperationPlan {
    #[must_use]
    pub fn needs_confirmation(&self) -> bool {
        todo!()
    }
}

#[must_use]
pub fn plan(_request: &PlanRequest) -> OperationPlan {
    todo!()
}

#[must_use]
pub fn is_protected(_location: &Location, _extra: &[Location]) -> bool {
    todo!()
}

#[cfg(test)]
mod tests;
