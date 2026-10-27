use super::{Flags, NO_PARENT, NameTable, TableError};
use crate::usn::FrnMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableChange {
    Add {
        file_id: u128,
        parent_id: u128,
        name: String,
        flags: Flags,
    },
    Remove {
        file_id: u128,
    },
    Rename {
        file_id: u128,
        parent_id: u128,
        name: String,
        flags: Flags,
    },
    Attributes {
        file_id: u128,
        flags: Flags,
    },
    Size {
        file_id: u128,
        bytes: u64,
    },
    JournalReset,
}

impl NameTable {
    pub fn apply(&mut self, change: TableChange, ids: &mut FrnMap) -> Result<(), TableError> {
        if self.apply_in_batch(change, ids)? > 1 {
            self.forget_removed(ids);
        }
        Ok(())
    }

    /// Returns how many entries the change removed; call `forget_removed` once the batch ends.
    pub fn apply_in_batch(
        &mut self,
        change: TableChange,
        ids: &mut FrnMap,
    ) -> Result<usize, TableError> {
        let mut removed = 0;
        match change {
            TableChange::Add {
                file_id,
                parent_id,
                name,
                flags,
            } => {
                let parent = self.resolve_parent(file_id, parent_id, ids)?;
                let previous = ids.get(&file_id).copied().filter(|&id| self.is_live(id));
                let id = self.upsert(previous, parent, (&name, flags))?;
                self.flags[id as usize] = flags.bits();
                ids.insert(file_id, id);
            }
            TableChange::Rename {
                file_id,
                parent_id,
                name,
                flags,
            } => {
                let id = self.resolve_file(file_id, ids)?;
                let parent = self.resolve_parent(file_id, parent_id, ids)?;
                self.rename(id, parent, &name)?;
                self.flags[id as usize] = flags.bits();
            }
            TableChange::Remove { file_id } => {
                removed = ids.remove(&file_id).map_or(0, |id| self.remove(id));
            }
            TableChange::Attributes { file_id, flags } => {
                let id = self.resolve_file(file_id, ids)?;
                self.flags[id as usize] = flags.bits();
            }
            TableChange::Size { file_id, bytes } => {
                let id = self.resolve_file(file_id, ids)?;
                self.set_size(id, bytes)?;
            }
            TableChange::JournalReset => {
                *self = Self::new();
                ids.clear();
            }
        }
        Ok(removed)
    }

    pub fn forget_removed(&self, ids: &mut FrnMap) {
        ids.retain(|_, id| self.is_live(*id));
    }

    fn upsert(
        &mut self,
        id: Option<u32>,
        parent: u32,
        (name, flags): (&str, Flags),
    ) -> Result<u32, TableError> {
        match id {
            Some(id) => {
                self.rename(id, parent, name)?;
                Ok(id)
            }
            None => self.push(parent, name, flags),
        }
    }

    fn resolve_file(&self, file_id: u128, ids: &FrnMap) -> Result<u32, TableError> {
        ids.get(&file_id)
            .copied()
            .filter(|&id| self.is_live(id))
            .ok_or(TableError::UnknownFileReference(file_id))
    }

    fn resolve_parent(
        &self,
        file_id: u128,
        parent_id: u128,
        ids: &FrnMap,
    ) -> Result<u32, TableError> {
        if parent_id == file_id {
            return Ok(NO_PARENT);
        }
        self.resolve_file(parent_id, ids)
    }
}
