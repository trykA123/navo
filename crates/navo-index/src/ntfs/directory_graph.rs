//! Folder-only dependency graph for two-pass NTFS directory insertion.

use super::{Flags, NtfsError, NtfsIndex, ROOT_REFERENCE, is_root};
use crate::table::{TableChange, TableError};
use crate::usn::FrnMap;

/// Directory metadata staged during pass 1 until parent-first table insertion.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct DirectoryRecord {
    pub(super) file_id: u128,
    pub(super) parent_id: u128,
    pub(super) name: String,
    pub(super) flags: Flags,
}

/// Stages pass-1 directories by FRN and inserts them into [`NtfsIndex`] parent-first.
#[derive(Default)]
pub(super) struct DirectoryGraph {
    by_frn: FrnMap,
    nodes: Vec<DirectoryRecord>,
    roots: Vec<DirectoryRecord>,
}

impl DirectoryGraph {
    /// Creates an empty directory graph.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Stages one directory record, normalizing root parent aliases and rejecting FRN conflicts.
    pub(super) fn push(&mut self, mut record: DirectoryRecord) -> Result<(), NtfsError> {
        if is_root(record.parent_id) {
            record.parent_id = ROOT_REFERENCE;
        }
        if is_root(record.file_id) {
            if let Some(prev) = self.roots.iter().find(|r| r.file_id == record.file_id) {
                return (prev == &record)
                    .then_some(())
                    .ok_or(NtfsError::Reenumerate);
            }
            self.roots.push(record);
            return Ok(());
        }
        if let Some(&slot) = self.by_frn.get(&record.file_id) {
            return (self.nodes[slot as usize] == record)
                .then_some(())
                .ok_or(NtfsError::Reenumerate);
        }
        let slot = u32::try_from(self.nodes.len()).map_err(|_| TableError::Full)?;
        self.by_frn.insert(record.file_id, slot);
        self.nodes.push(record);
        Ok(())
    }

    /// Inserts all staged directories into `index` parent-first, dropping graph scratch afterward.
    pub(super) fn insert_into(mut self, index: &mut NtfsIndex) -> Result<(), NtfsError> {
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
        // Three-colour marks: 0 = unvisited, 1 = visiting, 2 = inserted.
        let mut colors = vec![0u8; self.nodes.len()];
        let mut stack = Vec::new();
        for start in 0..self.nodes.len() {
            if colors[start] == 0 {
                colors[start] = 1;
                stack.push(start);
                self.drain_stack(&mut colors, &mut stack, index)?;
            }
        }
        Ok(())
    }

    /// Resolves the explicit folder stack via three-colour DFS, inserting parents first.
    fn drain_stack(
        &mut self,
        colors: &mut [u8],
        stack: &mut Vec<usize>,
        index: &mut NtfsIndex,
    ) -> Result<(), NtfsError> {
        while let Some(&curr) = stack.last() {
            match self.next_parent(self.nodes[curr].parent_id, index)? {
                Some(p) if colors[p] == 0 => {
                    colors[p] = 1;
                    stack.push(p);
                }
                Some(p) if colors[p] == 1 => return Err(NtfsError::Reenumerate),
                _ => {
                    let node = &mut self.nodes[curr];
                    let change = TableChange::Add {
                        file_id: node.file_id,
                        parent_id: node.parent_id,
                        name: std::mem::take(&mut node.name),
                        flags: node.flags | Flags::FOLDER,
                    };
                    index.table.apply_in_batch(change, &mut index.ids)?;
                    colors[curr] = 2;
                    stack.pop();
                }
            }
        }
        Ok(())
    }

    fn next_parent(&self, parent_id: u128, index: &NtfsIndex) -> Result<Option<usize>, NtfsError> {
        if let Some(&slot) = self.by_frn.get(&parent_id) {
            return Ok(Some(slot as usize));
        }
        if parent_id == ROOT_REFERENCE || index.ids.contains_key(&parent_id) {
            return Ok(None);
        }
        Err(NtfsError::Reenumerate)
    }
}
