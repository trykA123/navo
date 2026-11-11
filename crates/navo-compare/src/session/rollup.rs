//! Folder roll-up: flags and counts the differences below each folder row.

/// Least time between two sends of held (count-only) folder rows.
const HOLD: Duration = Duration::from_millis(100);

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use crate::model::{EntryKind, Row, Status};

struct Folder {
    row: Row,
    parent: Option<u64>,
}

/// The folder rows of one generation, with their parents.
///
/// Every call that counts a difference is a transition into a difference status: the row was
/// Checking or Same before, never already a difference. Counts only rise: a future path that
/// turns a difference into Same (sync, copy) must decrement.
#[derive(Default)]
pub(super) struct Rollup {
    folders: HashMap<u64, Folder>,
    /// Folders whose count changed but were not sent yet.
    dirty: BTreeSet<u64>,
    last_taken: Option<Instant>,
}

impl Rollup {
    /// Tracks the folder rows of a batch listed under `parent` and returns the ancestors the
    /// batch's differences newly flag. The caller sends the returned rows again.
    pub(super) fn add_batch(&mut self, rows: &[Row], parent: Option<u64>) -> Vec<Row> {
        let mut changed = Vec::new();
        let mut differs = 0;
        for row in rows {
            differs += u32::from(row.raw_status.is_difference());
            if is_folder(row) {
                let folder = Folder {
                    row: row.clone(),
                    parent,
                };
                self.folders.insert(row.id, folder);
            }
        }
        if differs > 0 {
            self.flag_from(parent, differs, &mut changed);
        }
        changed
    }

    /// A row listed under `parent` resolved to `status`.
    pub(super) fn resolve(&mut self, parent: Option<u64>, status: Status) -> Vec<Row> {
        let mut changed = Vec::new();
        if status.is_difference() {
            self.flag_from(parent, 1, &mut changed);
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
        row.differences_inside = folder.row.differences_inside;
        folder.row = row.clone();
        let parent = folder.parent;
        if row.raw_status.is_difference() && !previous.is_difference() {
            self.flag_from(parent, 1, &mut changed);
        }
        changed
    }

    /// Returns the held folder rows (each once, latest count) when `is_final` or when 100 ms
    /// passed since the last return; otherwise nothing.
    pub(super) fn take_dirty(&mut self, now: Instant, is_final: bool) -> Vec<Row> {
        let due = self
            .last_taken
            .is_none_or(|last| now.duration_since(last) >= HOLD);
        if self.dirty.is_empty() || !(is_final || due) {
            return Vec::new();
        }
        self.last_taken = Some(now);
        let ids = std::mem::take(&mut self.dirty);
        let folders = &self.folders;
        ids.iter()
            .filter_map(|id| folders.get(id))
            .map(|folder| folder.row.clone())
            .collect()
    }

    /// Adds `count` differences to every ancestor. An ancestor newly flagged goes to `changed`
    /// at once; the others are held as dirty for `take_dirty`.
    fn flag_from(&mut self, mut parent: Option<u64>, count: u32, changed: &mut Vec<Row>) {
        while let Some(id) = parent {
            let Some(folder) = self.folders.get_mut(&id) else {
                return;
            };
            folder.row.differences_inside += count;
            if folder.row.contains_differences {
                self.dirty.insert(id);
            } else {
                folder.row.contains_differences = true;
                self.dirty.remove(&id);
                changed.push(folder.row.clone());
            }
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
