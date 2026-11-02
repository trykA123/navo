mod build;
mod drop;
mod names;
mod protect;
mod risks;

use serde::{Deserialize, Serialize};

pub use build::{relative_folders, write_plan};
pub use drop::{DropKeys, DropOp, drop_op, right_button_drag};
pub use protect::is_protected;

use crate::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpKind {
    Copy,
    Move,
    Recycle,
    Delete,
    Rename,
    Link,
    Save,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileStamp {
    pub size: u64,
    pub modified_ms: Option<i64>,
    #[serde(with = "hash_hex")]
    pub hash: [u8; 32],
}

mod hash_hex {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    const HEX: &[u8; 16] = b"0123456789abcdef";

    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    pub(super) fn serialize<S: Serializer>(
        hash: &[u8; 32],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut text = String::with_capacity(64);
        for byte in hash {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        serializer.serialize_str(&text)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; 32], D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() != 64 {
            return Err(D::Error::invalid_length(
                text.len(),
                &"a 64-character hex hash",
            ));
        }
        let bytes = text.as_bytes();
        let mut hash = [0_u8; 32];
        for (at, slot) in hash.iter_mut().enumerate() {
            let high = nibble(bytes[at * 2])
                .ok_or_else(|| D::Error::custom("the file stamp hash is not hex"))?;
            let low = nibble(bytes[at * 2 + 1])
                .ok_or_else(|| D::Error::custom("the file stamp hash is not hex"))?;
            *slot = (high << 4) | low;
        }
        Ok(hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    MakeFolder {
        target: Location,
    },
    WriteFile {
        target: Location,
        staged: Location,
        staged_len: u64,
        expect: FileStamp,
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
    Overwrite(Location),
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
    plan_with(request, None)
}

#[must_use]
pub fn plan_keeping(request: &PlanRequest, base: &Location) -> OperationPlan {
    plan_with(request, Some(base))
}

fn plan_with(request: &PlanRequest, base: Option<&Location>) -> OperationPlan {
    let mut walk = build::Walk::new(request, base);
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
        (OpKind::Copy | OpKind::Move | OpKind::Link, None) | (OpKind::Rename | OpKind::Save, _) => {
        }
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
