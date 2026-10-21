use std::collections::HashSet;
use std::sync::Arc;

use rusty_core::{Location, OperationPlan, Step};
use rusty_fs::is_rename_temporary;

use crate::browser::OpLog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOffer {
    pub job: u64,
    pub plan: OperationPlan,
    pub done: Vec<u32>,
    pub created_at: i64,
    pub restore_pending: bool,
}

impl UndoOffer {
    #[must_use]
    pub fn items(&self) -> usize {
        self.item_targets().count()
    }

    pub(super) fn item_targets(&self) -> impl Iterator<Item = &Location> {
        let done: HashSet<u32> = self.done.iter().copied().collect();
        (0_u32..)
            .zip(&self.plan.steps)
            .filter(move |(seq, step)| done.contains(seq) && is_item(step))
            .filter_map(|(_, step)| match step {
                Step::Rename { target, .. } => Some(target),
                _ => None,
            })
    }
}

pub(super) fn is_item(step: &Step) -> bool {
    matches!(step, Step::Rename { target, .. } if !is_rename_temporary(target))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameReport {
    pub left: Vec<Location>,
    pub skipped: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedPreset {
    pub name: String,
    pub rules: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenamePresetList {
    pub named: Vec<NamedPreset>,
    pub last_used: Option<String>,
}

pub type LastRename = Arc<dyn Fn() -> Result<Option<UndoOffer>, String> + Send + Sync>;
/// Opens the log for undoing `offer.job`. The journal must mark that job restore-pending before
/// this returns, so a crash mid-undo is finished at start. `undone(seq, Done | Skipped)` marks the
/// step undone; `undone(seq, Failed)` leaves it. `finish(Done)` marks the job undone; any other
/// end, or a failed step, leaves it restore-pending.
pub type UndoLogFactory = Arc<dyn Fn(&UndoOffer) -> Result<Box<dyn OpLog>, String> + Send + Sync>;
pub type ListPresets = Arc<dyn Fn() -> Result<RenamePresetList, String> + Send + Sync>;
pub type SavePreset = Arc<dyn Fn(&str, &str) -> Result<(), String> + Send + Sync>;
pub type DeletePreset = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct RenameHooks {
    pub last: LastRename,
    pub undo_log: UndoLogFactory,
    pub presets: ListPresets,
    pub save: SavePreset,
    pub delete: DeletePreset,
}
