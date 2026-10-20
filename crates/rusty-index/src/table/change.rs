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
    JournalReset,
}

impl NameTable {
    pub fn apply(&mut self, change: TableChange, ids: &mut FrnMap) -> Result<(), TableError> {
        match change {
            TableChange::Add {
                file_id,
                parent_id,
                name,
                flags,
            } => {
                let parent = self.resolve_parent(file_id, parent_id, ids)?;
                let previous = ids.get(&file_id).copied().filter(|&id| self.is_live(id));
                let id = self.upsert(previous, parent, &name)?;
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
            TableChange::Remove { file_id } => self.remove_file(file_id, ids),
            TableChange::Attributes { file_id, flags } => {
                let id = self.resolve_file(file_id, ids)?;
                self.flags[id as usize] = flags.bits();
            }
            TableChange::JournalReset => {
                *self = Self::new();
                ids.clear();
            }
        }
        Ok(())
    }

    fn upsert(&mut self, id: Option<u32>, parent: u32, name: &str) -> Result<u32, TableError> {
        match id {
            Some(id) => {
                self.rename(id, parent, name)?;
                Ok(id)
            }
            None => self.push(parent, name, Flags::NONE),
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

    fn remove_file(&mut self, file_id: u128, ids: &mut FrnMap) {
        if let Some(id) = ids.remove(&file_id)
            && self.remove(id) > 1
        {
            ids.retain(|_, id| self.is_live(*id));
        }
    }
}
