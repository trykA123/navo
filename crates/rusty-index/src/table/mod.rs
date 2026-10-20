mod flags;

use std::ops::Range;

use crate::fuzzy::{Fuzzy, Match};

pub use flags::Flags;

pub const NO_PARENT: u32 = u32::MAX;
const MAX_DEPTH: usize = 4096;
const NAME_END: char = '\0';

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("the name table is full")]
    Full,
    #[error("entry {0} does not exist")]
    UnknownId(u32),
    #[error("entry {0} would become its own ancestor")]
    Cycle(u32),
}

#[derive(Debug, Clone, Default)]
pub struct NameTable {
    names: String,
    offsets: Vec<u32>,
    parents: Vec<u32>,
    flags: Vec<u16>,
    live: usize,
}

fn clean(name: &str) -> &str {
    name.split(NAME_END).next().unwrap_or_default()
}

impl NameTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(entries: usize, name_bytes: usize) -> Self {
        Self {
            names: String::with_capacity(name_bytes),
            offsets: Vec::with_capacity(entries),
            parents: Vec::with_capacity(entries),
            flags: Vec::with_capacity(entries),
            live: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn slots(&self) -> u32 {
        self.offsets.len() as u32
    }

    pub fn memory_bytes(&self) -> usize {
        self.names.capacity()
            + self.offsets.capacity() * size_of::<u32>()
            + self.parents.capacity() * size_of::<u32>()
            + self.flags.capacity() * size_of::<u16>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.names.shrink_to_fit();
        self.offsets.shrink_to_fit();
        self.parents.shrink_to_fit();
        self.flags.shrink_to_fit();
    }

    pub fn push(&mut self, parent: u32, name: &str, flags: Flags) -> Result<u32, TableError> {
        let id = self.slots();
        if id == NO_PARENT {
            return Err(TableError::Full);
        }
        if parent == id {
            return Err(TableError::Cycle(id));
        }
        let offset = self.append_name(clean(name))?;
        self.offsets.push(offset);
        self.parents.push(parent);
        self.flags.push(Flags::from_stored(flags.bits()).bits());
        self.live += 1;
        Ok(id)
    }

    pub fn rename(&mut self, id: u32, parent: u32, name: &str) -> Result<(), TableError> {
        if !self.is_live(id) {
            return Err(TableError::UnknownId(id));
        }
        if parent == id || self.is_ancestor(id, parent) {
            return Err(TableError::Cycle(id));
        }
        let name = clean(name);
        if self.name(id) != Some(name) {
            self.offsets[id as usize] = self.append_name(name)?;
        }
        self.parents[id as usize] = parent;
        Ok(())
    }

    pub fn remove(&mut self, id: u32) -> usize {
        if !self.is_live(id) {
            return 0;
        }
        let was_folder = self.flags[id as usize] & Flags::FOLDER.bits() != 0;
        self.mark_deleted(id);
        if was_folder {
            1 + self.remove_orphans()
        } else {
            1
        }
    }

    pub fn compact(&mut self) -> Vec<Option<u32>> {
        let slots = self.slots();
        let mut map = vec![None; slots as usize];
        let mut next = 0u32;
        for id in 0..slots {
            if self.is_live(id) {
                map[id as usize] = Some(next);
                next += 1;
            }
        }
        let mut fresh = Self::with_capacity(next as usize, self.names.len());
        for id in 0..slots {
            if map[id as usize].is_none() {
                continue;
            }
            let parent = map
                .get(self.parents[id as usize] as usize)
                .copied()
                .flatten()
                .unwrap_or(NO_PARENT);
            fresh.offsets.push(fresh.names.len() as u32);
            fresh.names.push_str(self.name_unchecked(id));
            fresh.names.push(NAME_END);
            fresh.parents.push(parent);
            fresh.flags.push(self.flags[id as usize]);
        }
        fresh.live = next as usize;
        fresh.shrink_to_fit();
        *self = fresh;
        map
    }

    pub fn name(&self, id: u32) -> Option<&str> {
        self.is_live(id).then(|| self.name_unchecked(id))
    }

    pub fn flags(&self, id: u32) -> Option<Flags> {
        self.is_live(id)
            .then(|| Flags::from_stored(self.flags[id as usize]))
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        let parent = *self.parents.get(id as usize)?;
        self.is_live(id)
            .then_some(parent)
            .filter(|&p| self.is_live(p))
    }

    pub fn path(&self, id: u32) -> Option<String> {
        self.path_with(id, std::path::MAIN_SEPARATOR)
    }

    pub fn path_with(&self, id: u32, separator: char) -> Option<String> {
        if !self.is_live(id) {
            return None;
        }
        let mut parts = Vec::new();
        let mut current = Some(id);
        while let Some(at) = current {
            if parts.len() >= MAX_DEPTH {
                return None;
            }
            parts.push(self.name_unchecked(at));
            current = self.parent(at);
        }
        parts.reverse();
        Some(parts.join(separator.encode_utf8(&mut [0; 4])))
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.iter_range(0..self.slots())
    }

    pub fn iter_range(&self, range: Range<u32>) -> impl Iterator<Item = (u32, &str)> {
        let end = range.end.min(self.slots());
        (range.start..end).filter_map(|id| self.name(id).map(|name| (id, name)))
    }

    pub fn scan<'a>(
        &'a self,
        range: Range<u32>,
        fuzzy: &'a mut Fuzzy,
    ) -> impl Iterator<Item = (u32, Match)> + 'a {
        self.iter_range(range)
            .filter_map(|(id, name)| fuzzy.score(name).map(|hit| (id, hit)))
    }

    fn is_live(&self, id: u32) -> bool {
        self.flags
            .get(id as usize)
            .is_some_and(|bits| bits & Flags::DELETED == 0)
    }

    fn remove_orphans(&mut self) -> usize {
        let orphans: Vec<u32> = (0..self.slots())
            .filter(|&id| self.is_live(id) && self.has_deleted_ancestor(id))
            .collect();
        for &id in &orphans {
            self.mark_deleted(id);
        }
        orphans.len()
    }

    fn mark_deleted(&mut self, id: u32) {
        self.flags[id as usize] |= Flags::DELETED;
        self.live -= 1;
    }

    fn append_name(&mut self, name: &str) -> Result<u32, TableError> {
        let offset = u32::try_from(self.names.len()).map_err(|_| TableError::Full)?;
        let end = self.names.len() + name.len() + NAME_END.len_utf8();
        u32::try_from(end).map_err(|_| TableError::Full)?;
        self.names.push_str(name);
        self.names.push(NAME_END);
        Ok(offset)
    }

    fn name_unchecked(&self, id: u32) -> &str {
        let rest = self
            .names
            .get(self.offsets[id as usize] as usize..)
            .unwrap_or_default();
        rest.split(NAME_END).next().unwrap_or_default()
    }

    fn is_ancestor(&self, ancestor: u32, of: u32) -> bool {
        let mut current = of;
        for _ in 0..MAX_DEPTH {
            if current == ancestor {
                return true;
            }
            match self.parents.get(current as usize) {
                Some(&parent) => current = parent,
                None => return false,
            }
        }
        false
    }

    fn has_deleted_ancestor(&self, id: u32) -> bool {
        let mut current = self.parents[id as usize];
        for _ in 0..MAX_DEPTH {
            match self.flags.get(current as usize) {
                Some(bits) if bits & Flags::DELETED != 0 => return true,
                Some(_) => current = self.parents[current as usize],
                None => return false,
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
