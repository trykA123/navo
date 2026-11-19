//! Folder-only dependency graph for two-pass NTFS directory insertion.

use super::{Flags, NtfsError, NtfsIndex, ROOT_REFERENCE, is_folder, is_root};
use crate::table::{TableChange, TableError};
use crate::usn::{FrnMap, UsnRecord};

pub(super) const MAX_LATE_FOLDERS: usize = 1_000_000;

/// Directory metadata staged during pass 1 until parent-first table insertion.
#[derive(Default, PartialEq, Eq)]
pub(super) struct DirectoryRecord {
    pub(super) file_id: u128,
    pub(super) parent_id: u128,
    pub(super) name: String,
    pub(super) flags: Flags,
}

impl From<UsnRecord> for DirectoryRecord {
    /// Keeps the directory metadata needed for parent-first insertion.
    fn from(record: UsnRecord) -> Self {
        Self {
            file_id: record.file_id,
            parent_id: record.parent_id,
            name: record.name,
            flags: record.flags,
        }
    }
}

/// An unreachable folder and whether a cycle blocked its parent chain.
pub(super) struct UnplacedFolder {
    pub(super) record: DirectoryRecord,
    pub(super) cycle: bool,
}

#[derive(Default)]
pub(super) struct DirectoryResolution {
    pub(super) unplaced: Vec<UnplacedFolder>,
    pub(super) skipped: u64,
    pub(super) held_cap: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unseen,
    Visiting,
    Settled,
    CycleSkipped,
}

/// Stages directories by FRN and inserts them into [`NtfsIndex`] parent-first.
#[derive(Default)]
pub(super) struct DirectoryGraph {
    by_frn: FrnMap,
    nodes: Vec<DirectoryRecord>,
    roots: Vec<DirectoryRecord>,
    resolution: DirectoryResolution,
}

impl DirectoryGraph {
    /// Creates an empty directory graph.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Stages one directory record, keeping the first record for each FRN.
    pub(super) fn push(&mut self, mut record: DirectoryRecord) -> Result<(), NtfsError> {
        if is_root(record.parent_id) {
            record.parent_id = ROOT_REFERENCE;
        }
        if is_root(record.file_id) {
            if let Some(prev) = self.roots.iter().find(|r| r.file_id == record.file_id) {
                self.resolution.skipped += u64::from(prev != &record);
                return Ok(());
            }
            self.roots.push(record);
            return Ok(());
        }
        if let Some(&slot) = self.by_frn.get(&record.file_id) {
            self.resolution.skipped += u64::from(self.nodes[slot as usize] != record);
            return Ok(());
        }
        let slot = u32::try_from(self.nodes.len()).map_err(|_| TableError::Full)?;
        self.by_frn.insert(record.file_id, slot);
        self.nodes.push(record);
        Ok(())
    }

    /// Inserts reachable folders parent-first and reports the capped set of unplaced folders.
    pub(super) fn insert_into(
        mut self,
        index: &mut NtfsIndex,
    ) -> Result<DirectoryResolution, NtfsError> {
        let root = *index
            .ids
            .get(&ROOT_REFERENCE)
            .ok_or(NtfsError::InvalidResponse("missing volume root"))?;
        for entry in std::mem::take(&mut self.roots) {
            index.ids.insert(entry.file_id, root);
            let flags = entry.flags | Flags::FOLDER;
            let change = TableChange::Attributes {
                file_id: ROOT_REFERENCE,
                flags,
            };
            index.table.apply_in_batch(change, &mut index.ids)?;
        }
        let mut visits = vec![Visit::Unseen; self.nodes.len()];
        let mut stack = Vec::new();
        for start in 0..self.nodes.len() {
            if visits[start] == Visit::Unseen {
                visits[start] = Visit::Visiting;
                stack.push(start);
                self.drain_stack(&mut visits, &mut stack, index)?;
            }
        }
        Ok(self.resolution)
    }

    /// Resolves the folder stack parent-first, retaining chains that reach no indexed folder.
    fn drain_stack(
        &mut self,
        visits: &mut [Visit],
        stack: &mut Vec<usize>,
        index: &mut NtfsIndex,
    ) -> Result<(), NtfsError> {
        while let Some(&curr) = stack.last() {
            let parent_id = self.nodes[curr].parent_id;
            let parent = self.next_parent(parent_id);
            if let Some(parent) = parent.filter(|&slot| visits[slot] == Visit::Unseen) {
                visits[parent] = Visit::Visiting;
                stack.push(parent);
                continue;
            }
            // A missing parent, cycle, or previously skipped parent leaves the stack unreachable.
            if !is_folder(index, parent_id) {
                let parent_visit = parent.map(|slot| visits[slot]);
                let cycle = matches!(parent_visit, Some(Visit::Visiting | Visit::CycleSkipped));
                self.stash_unplaced(visits, stack, cycle);
                break;
            }
            let node = &mut self.nodes[curr];
            let change = TableChange::Add {
                file_id: node.file_id,
                parent_id,
                name: std::mem::take(&mut node.name),
                flags: node.flags | Flags::FOLDER,
            };
            index.table.apply_in_batch(change, &mut index.ids)?;
            visits[curr] = Visit::Settled;
            stack.pop();
        }
        Ok(())
    }

    /// Holds the unreachable chain and marks every slot for future parent lookups.
    fn stash_unplaced(&mut self, visits: &mut [Visit], stack: &mut Vec<usize>, cycle: bool) {
        let visit = if cycle {
            Visit::CycleSkipped
        } else {
            Visit::Settled
        };
        for slot in stack.drain(..) {
            visits[slot] = visit;
            let record = std::mem::take(&mut self.nodes[slot]);
            if self.resolution.unplaced.len() < MAX_LATE_FOLDERS {
                self.resolution
                    .unplaced
                    .push(UnplacedFolder { record, cycle });
            } else {
                self.resolution.skipped += 1;
                self.resolution.held_cap = true;
            }
        }
    }

    /// Looks up the parent in the staged directories.
    fn next_parent(&self, parent_id: u128) -> Option<usize> {
        self.by_frn.get(&parent_id).map(|&slot| slot as usize)
    }
}
