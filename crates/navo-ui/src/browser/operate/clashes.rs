//! Facts for the conflict list: the name, size and modified time of the first conflicts' sources
//! and of the items already at their targets, read on the planning thread.

use navo_core::{EntryKind, OperationPlan};
use navo_fs::{FsEntry, ProviderOps};

/// How many conflicts the list shows; the rest are only counted.
pub const SHOWN: usize = 200;

/// One side of a conflict as the disk reported it; `None` fields when it could not be read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub size: Option<u64>,
    pub modified_ms: Option<i64>,
    pub is_folder: bool,
}

/// One row of the conflict list: the clashing name, the item being copied and the one already
/// there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    pub name: String,
    pub source: Facts,
    pub target: Facts,
}

/// Facts of an entry the provider read.
fn facts(entry: &FsEntry) -> Facts {
    let is_folder = entry.kind == EntryKind::Folder;
    Facts {
        size: (!is_folder).then_some(entry.size),
        modified_ms: entry.modified_ms,
        is_folder,
    }
}

/// Reads the facts of the first `SHOWN` conflicts of `plan`; a side that cannot be read stays
/// unknown, so one unreadable file never hides the list.
pub(super) fn clashes(provider: &dyn ProviderOps, plan: &OperationPlan) -> Vec<Clash> {
    let read = |location| {
        provider
            .metadata(location)
            .map(|entry| facts(&entry))
            .unwrap_or_default()
    };
    plan.conflicts
        .iter()
        .take(SHOWN)
        .map(|conflict| Clash {
            name: conflict.target.name().map_or_else(
                || conflict.target.as_str().to_owned(),
                |n| n.display().into_owned(),
            ),
            source: read(&conflict.source),
            target: read(&conflict.target),
        })
        .collect()
}
