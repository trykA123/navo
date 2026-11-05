use serde::{Deserialize, Serialize};

use super::{Flags, NO_PARENT, NameTable, StoredFlags, TableError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub bytes: u64,
    pub files: u32,
    pub folders: u32,
}

impl NameTable {
    pub fn size(&self, id: u32) -> Option<u64> {
        self.is_live(id).then(|| self.bytes[id as usize])
    }

    pub fn totals(&self, id: u32) -> Option<Totals> {
        self.flags(id)?.contains(Flags::FOLDER).then(|| {
            let [files, folders] = self.folder_counts(id);
            Totals {
                bytes: self.bytes[id as usize],
                files,
                folders,
            }
        })
    }

    pub fn set_size(&mut self, id: u32, bytes: u64) -> Result<(), TableError> {
        let flags = self.flags(id).ok_or(TableError::UnknownId(id))?;
        if flags.contains(Flags::FOLDER) {
            return Ok(());
        }
        let old = std::mem::replace(&mut self.bytes[id as usize], bytes);
        self.adjust_ancestors(
            self.parents[id as usize],
            Totals {
                bytes: bytes.abs_diff(old),
                ..Totals::default()
            },
            bytes >= old,
        );
        Ok(())
    }

    /// Writes `flags`, keeping the folder counts column and ancestor totals in step.
    pub(super) fn set_flags(&mut self, id: u32, flags: Flags) {
        let at = id as usize;
        let was_folder = self.flags[at] & Flags::FOLDER.stored() != 0;
        let is_folder = flags.stored() & Flags::FOLDER.stored() != 0;
        if was_folder == is_folder {
            self.flags[at] = flags.stored();
        } else if is_folder {
            self.promote_to_folder(id, flags);
        } else if self.children.of(id).next().is_none() {
            self.demote_empty_folder(id, flags);
        } else {
            // A demoted folder with live children is refused on load; keep its totals as today.
            self.flags[at] = flags.stored();
        }
    }

    fn promote_to_folder(&mut self, id: u32, flags: Flags) {
        let at = id as usize;
        let parent = self.parents[at];
        let descendants = self.child_totals(id);
        let old = self.contribution(id);
        self.adjust_ancestors(parent, old, false);
        let child_counts = Totals {
            files: descendants.files,
            folders: descendants.folders,
            ..Totals::default()
        };
        self.adjust_ancestors(parent, child_counts, false);
        self.bytes[at] = descendants.bytes;
        self.flags[at] = flags.stored();
        self.put_counts(id, [descendants.files, descendants.folders]);
        let new = self.contribution(id);
        self.adjust_ancestors(parent, new, true);
    }

    fn demote_empty_folder(&mut self, id: u32, flags: Flags) {
        let at = id as usize;
        let parent = self.parents[at];
        let old = self.contribution(id);
        self.adjust_ancestors(parent, old, false);
        self.take_counts(id);
        self.flags[at] = flags.stored();
        let new = self.contribution(id);
        self.adjust_ancestors(parent, new, true);
    }

    fn child_totals(&self, id: u32) -> Totals {
        let mut totals = Totals::default();
        for child in self.children.of(id) {
            let child = self.contribution(child);
            totals.bytes += child.bytes;
            totals.files += child.files;
            totals.folders += child.folders;
        }
        totals
    }

    fn put_counts(&mut self, id: u32, counts: [u32; 2]) {
        match self.counts.binary_search_by_key(&id, |&(slot, _)| slot) {
            Ok(at) => self.counts[at].1 = counts,
            Err(at) => {
                super::growth::grow_column(&mut self.counts);
                self.counts.insert(at, (id, counts));
            }
        }
    }

    fn take_counts(&mut self, id: u32) {
        if let Ok(at) = self.counts.binary_search_by_key(&id, |&(slot, _)| slot) {
            self.counts.remove(at);
        }
    }

    pub(super) fn contribution(&self, id: u32) -> Totals {
        let folder = self
            .flags(id)
            .is_some_and(|flags| flags.contains(Flags::FOLDER));
        let [files, folders] = if folder {
            self.folder_counts(id)
        } else {
            [0; 2]
        };
        Totals {
            bytes: self.bytes[id as usize],
            files: files + u32::from(!folder),
            folders: folders + u32::from(folder),
        }
    }

    pub(super) fn adjust_ancestors(&mut self, mut id: u32, totals: Totals, add: bool) {
        for _ in 0..self.slots() {
            if !self.is_live(id) {
                break;
            }
            let at = id as usize;
            if add {
                self.bytes[at] = self.bytes[at].saturating_add(totals.bytes);
            } else {
                self.bytes[at] = self.bytes[at].saturating_sub(totals.bytes);
            }
            if let Some(counts) = self.folder_counts_mut(id) {
                adjust_counts(counts, totals, add);
            }
            id = self.parents[at];
        }
    }

    pub(super) fn folder_counts(&self, id: u32) -> [u32; 2] {
        if self.flags[id as usize] & Flags::FOLDER.stored() == 0 {
            return [0; 2];
        }
        self.counts
            .binary_search_by_key(&id, |&(slot, _)| slot)
            .map_or([0; 2], |at| self.counts[at].1)
    }

    pub(super) fn folder_counts_mut(&mut self, id: u32) -> Option<&mut [u32; 2]> {
        if self.flags[id as usize] & Flags::FOLDER.stored() == 0 {
            return None;
        }
        let at = self
            .counts
            .binary_search_by_key(&id, |&(slot, _)| slot)
            .ok()?;
        Some(&mut self.counts[at].1)
    }

    pub(crate) fn stored_totals(&self) -> (&[u64], impl Iterator<Item = [u32; 2]> + '_) {
        (
            &self.bytes,
            (0..self.slots()).map(|id| self.folder_counts(id)),
        )
    }

    pub(crate) fn restore_totals(&mut self, bytes: Vec<u64>, counts: Vec<[u32; 2]>) -> bool {
        self.bytes = bytes;
        // Only live folders get rebuilt, so only their stored sums need keeping for the check.
        let folders: Vec<u32> = self.counted_folders().collect();
        let stored: Vec<u64> = folders.iter().map(|&id| self.bytes[id as usize]).collect();
        if !self.rebuild_totals() {
            return false;
        }
        folders.iter().map(|&id| self.bytes[id as usize]).eq(stored)
            && (0..self.slots())
                .map(|id| self.folder_counts(id))
                .eq(counts)
    }

    fn counted_folders(&self) -> impl Iterator<Item = u32> + '_ {
        self.live_ids()
            .filter(|&id| self.flags[id as usize] & Flags::FOLDER.stored() != 0)
    }

    /// Recomputes folder sums by a post-order walk of the child lists, with no per-slot scratch.
    /// Returns `false` when the lists do not form one tree over every live entry.
    pub(super) fn rebuild_totals(&mut self) -> bool {
        let live_folder = |bits: &StoredFlags| {
            bits & (Flags::FOLDER.stored() | Flags::DELETED) == Flags::FOLDER.stored()
        };
        let folders = self.flags.iter().filter(|bits| live_folder(bits)).count();
        self.counts.clear();
        if self.counts.try_reserve_exact(folders).is_err() {
            return false;
        }
        self.counts.extend(
            (0u32..)
                .zip(&self.flags)
                .filter(|(_, bits)| live_folder(bits))
                .map(|(id, _)| (id, [0; 2])),
        );
        for &(id, _) in &self.counts {
            self.bytes[id as usize] = 0;
        }
        if self.slots() == 0 || (!self.is_live(0) && self.live == 0) {
            return true;
        }
        if !self.is_live(0) || self.parents[0] != NO_PARENT {
            return false;
        }
        self.walk_totals()
    }

    fn walk_totals(&mut self) -> bool {
        // Each live entry is entered once and left once.
        let mut steps = 2 * self.offsets.len() + 2;
        let (mut id, mut visited, mut descend) = (0u32, 0usize, true);
        while take_step(&mut steps) {
            match descend.then(|| self.children.first_child(id)).flatten() {
                Some(child) if self.linked_under(child, id) => {
                    id = child;
                    continue;
                }
                Some(_) => return false,
                None => {}
            }
            visited += 1;
            if id == 0 {
                return visited == self.live;
            }
            let parent = self.parents[id as usize];
            self.add_to_parent(id, parent);
            match self.children.next_sibling(id) {
                Some(next) if self.linked_under(next, parent) => (id, descend) = (next, true),
                Some(_) => return false,
                None => (id, descend) = (parent, false),
            }
        }
        false
    }

    fn linked_under(&self, child: u32, parent: u32) -> bool {
        self.is_live(child) && self.parents[child as usize] == parent
    }

    /// Saturating, so a corrupt list that revisits entries cannot overflow before it is caught.
    fn add_to_parent(&mut self, id: u32, parent: u32) {
        let at = id as usize;
        let folder = self.flags[at] & Flags::FOLDER.stored() != 0;
        let [files, folders] = self.folder_counts(id);
        let bytes = self.bytes[at];
        self.bytes[parent as usize] = self.bytes[parent as usize].saturating_add(bytes);
        if let Some(counts) = self.folder_counts_mut(parent) {
            counts[0] = counts[0]
                .saturating_add(files)
                .saturating_add(u32::from(!folder));
            counts[1] = counts[1]
                .saturating_add(folders)
                .saturating_add(u32::from(folder));
        }
    }
}

fn take_step(steps: &mut usize) -> bool {
    *steps = steps.saturating_sub(1);
    *steps > 0
}

fn adjust_counts(counts: &mut [u32; 2], totals: Totals, add: bool) {
    if add {
        counts[0] += totals.files;
        counts[1] += totals.folders;
    } else {
        counts[0] = counts[0].saturating_sub(totals.files);
        counts[1] = counts[1].saturating_sub(totals.folders);
    }
}

#[cfg(test)]
#[path = "totals_tests.rs"]
mod tests;
