//! Folder roll-up: turns `contains_differences` on for the folders above a difference.

use std::collections::HashMap;

use crate::model::{EntryKind, Row, Status};

struct Folder {
    row: Row,
    parent: Option<u64>,
}

/// The folder rows of one generation, with their parents.
#[derive(Default)]
pub(super) struct Rollup {
    folders: HashMap<u64, Folder>,
}

impl Rollup {
    /// Tracks the folder rows of a batch listed under `parent` and returns the ancestors the
    /// batch's differences newly flag. The caller sends the returned rows again.
    pub(super) fn add_batch(&mut self, rows: &[Row], parent: Option<u64>) -> Vec<Row> {
        let mut changed = Vec::new();
        let mut differs = false;
        for row in rows {
            differs = differs || row.raw_status.is_difference();
            if is_folder(row) {
                let folder = Folder {
                    row: row.clone(),
                    parent,
                };
                self.folders.insert(row.id, folder);
            }
        }
        if differs {
            self.flag_from(parent, &mut changed);
        }
        changed
    }

    /// A row listed under `parent` resolved to `status`.
    pub(super) fn resolve(&mut self, parent: Option<u64>, status: Status) -> Vec<Row> {
        let mut changed = Vec::new();
        if status.is_difference() {
            self.flag_from(parent, &mut changed);
        }
        changed
    }

    /// A folder row got a new status (its listing failed). Keeps the folder's own flag on `row`.
    pub(super) fn update_folder(&mut self, row: &mut Row, previous: Status) -> Vec<Row> {
        let mut changed = Vec::new();
        let Some(folder) = self.folders.get_mut(&row.id) else {
            return changed;
        };
        row.contains_differences = folder.row.contains_differences;
        folder.row = row.clone();
        let parent = folder.parent;
        if row.raw_status.is_difference() && !previous.is_difference() {
            self.flag_from(parent, &mut changed);
        }
        changed
    }

    fn flag_from(&mut self, mut parent: Option<u64>, changed: &mut Vec<Row>) {
        while let Some(folder) = parent.and_then(|id| self.folders.get_mut(&id)) {
            if folder.row.contains_differences {
                return;
            }
            folder.row.contains_differences = true;
            changed.push(folder.row.clone());
            parent = folder.parent;
        }
    }
}

fn is_folder(row: &Row) -> bool {
    [row.left.as_deref(), row.right.as_deref()]
        .into_iter()
        .flatten()
        .any(|entry| entry.kind == EntryKind::Folder)
}
