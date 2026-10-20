mod build;
mod drop;
mod names;
mod protect;
mod risks;

use serde::{Deserialize, Serialize};

pub use drop::{DropKeys, DropOp, drop_op};
pub use protect::is_protected;

use crate::Location;

pub const TEMP_SUFFIX: &str = ".ferrite-part";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpKind {
    Copy,
    Move,
    Recycle,
    Delete,
    Rename,
    Link,
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
    MakeLink {
        target_of: Location,
        at: Location,
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
        !self.risks.is_empty()
    }
}

#[must_use]
pub fn plan(request: &PlanRequest) -> OperationPlan {
    let mut walk = build::Walk::new(request);
    match (&request.kind, &request.target) {
        (OpKind::Copy, Some(target)) => walk.copy(target),
        (OpKind::Move, Some(target)) if request.same_volume => walk.rename_tops(target),
        (OpKind::Move, Some(target)) => {
            walk.copy(target);
            walk.remove_moved();
        }
        (OpKind::Link, Some(target)) => walk.link_tops(target),
        (OpKind::Recycle, _) => recycle(request, &mut walk),
        (OpKind::Delete, _) => {
            walk.steps = request.items.iter().rev().map(build::removal).collect();
        }
        (OpKind::Copy | OpKind::Move | OpKind::Link, None) | (OpKind::Rename, _) => {}
    }
    let totals = risks::totals(request, &walk.steps, &walk.moved);
    let risks = risks::risks(request, &walk, &totals);
    OperationPlan {
        kind: request.kind,
        steps: walk.steps,
        totals,
        conflicts: walk.conflicts,
        risks,
    }
}

fn recycle(request: &PlanRequest, walk: &mut build::Walk<'_>) {
    for (index, item) in request.items.iter().enumerate() {
        if item.depth == 0 {
            walk.steps.push(Step::Recycle {
                location: item.location.clone(),
            });
            walk.moved[index] = true;
        }
    }
}

#[cfg(test)]
mod tests;
