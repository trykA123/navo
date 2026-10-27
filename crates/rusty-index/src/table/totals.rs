use serde::{Deserialize, Serialize};

use super::{Flags, NameTable, TableError};

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
        if self.flags[id as usize] & Flags::FOLDER.bits() == 0 {
            return [0; 2];
        }
        self.counts
            .binary_search_by_key(&id, |&(slot, _)| slot)
            .map_or([0; 2], |at| self.counts[at].1)
    }

    pub(super) fn folder_counts_mut(&mut self, id: u32) -> Option<&mut [u32; 2]> {
        if self.flags[id as usize] & Flags::FOLDER.bits() == 0 {
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
        let expected = self.bytes.clone();
        self.rebuild_totals();
        self.bytes == expected
            && (0..self.slots())
                .map(|id| self.folder_counts(id))
                .eq(counts)
    }

    pub(super) fn rebuild_totals(&mut self) {
        self.counts = self
            .live_ids()
            .filter(|&id| self.flags[id as usize] & Flags::FOLDER.bits() != 0)
            .map(|id| (id, [0; 2]))
            .collect();
        for id in 0..self.slots() {
            if self
                .flags(id)
                .is_some_and(|flags| flags.contains(Flags::FOLDER))
            {
                self.bytes[id as usize] = 0;
            }
        }
        let mut remaining: Vec<_> = (0..self.slots())
            .map(|id| self.children.of(id).count())
            .collect();
        let mut ready: Vec<_> = self
            .live_ids()
            .filter(|&id| remaining[id as usize] == 0)
            .collect();
        while let Some(id) = ready.pop() {
            let Some(parent) = self.parent(id) else {
                continue;
            };
            let totals = self.contribution(id);
            let at = parent as usize;
            self.bytes[at] = self.bytes[at].saturating_add(totals.bytes);
            if let Some(counts) = self.folder_counts_mut(parent) {
                counts[0] += totals.files;
                counts[1] += totals.folders;
            }
            remaining[at] -= 1;
            if remaining[at] == 0 {
                ready.push(parent);
            }
        }
    }
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
