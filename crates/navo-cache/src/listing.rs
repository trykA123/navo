use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::time::SystemTime;

use navo_core::ProviderPath;
use navo_fs::FsEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedListing {
    pub rows: Arc<[FsEntry]>,
    pub read_at: SystemTime,
    pub folder_changed: Option<SystemTime>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(FsEntry),
    Removed(ProviderPath),
    Renamed { from: ProviderPath, to: FsEntry },
    Modified(FsEntry),
}

impl CachedListing {
    pub(crate) fn estimated_bytes(&self) -> usize {
        size_of::<Self>() + self.rows.iter().map(row_bytes).sum::<usize>()
    }

    #[cfg(test)]
    pub(crate) fn patched(&self, changes: &[Change], changed_at: SystemTime) -> Self {
        self.patched_with(changes, changed_at, None).0
    }

    /// Applies watcher changes and returns the next generation with the lookup that indexes it.
    pub(crate) fn patched_with(
        &self,
        changes: &[Change],
        changed_at: SystemTime,
        lookup: Option<NameLookup>,
    ) -> (Self, Option<NameLookup>) {
        if changes.is_empty() {
            return (self.clone(), lookup);
        }
        let lookup = lookup.or_else(|| NameLookup::build(&self.rows));
        let (rows, lookup) = match lookup {
            Some(lookup) => match lookup.patch(&self.rows, changes) {
                Some((rows, lookup)) => (rows, Some(lookup)),
                None => (indexed_rows(&self.rows, changes), None),
            },
            None => (indexed_rows(&self.rows, changes), None),
        };
        let listing = Self {
            rows,
            read_at: changed_at.max(self.read_at),
            ..self.clone()
        };
        (listing, lookup)
    }
}

fn held(name: &ProviderPath, vacated: &[&ProviderPath]) -> bool {
    vacated.contains(&name)
}

fn next_at(compact_len: usize, kept: bool) -> Option<u32> {
    if !kept {
        return Some(u32::MAX);
    }
    u32::try_from(compact_len).ok()
}

fn indexed_rows(rows: &[FsEntry], changes: &[Change]) -> Arc<[FsEntry]> {
    let mut indexed = IndexedRows::new(rows);
    for change in changes {
        indexed.apply(change);
    }
    indexed.into_rows()
}

// Key is the seeded hash of the row name; the row at the value is checked by name on every find.
pub(crate) struct NameLookup {
    seed: RandomState,
    map: HashMap<u64, u32>,
    #[cfg(test)]
    constant: bool,
}

impl NameLookup {
    pub(crate) fn build(rows: &[FsEntry]) -> Option<Self> {
        Self::from_parts(rows, Self::fresh(rows))
    }

    pub(crate) fn find(&self, rows: &[FsEntry], name: &ProviderPath) -> Option<u32> {
        let at = *self.map.get(&self.key(name))?;
        let row = rows.get(usize::try_from(at).ok()?)?;
        (row.name == *name).then_some(at)
    }

    pub(crate) fn bytes(&self) -> usize {
        size_of::<Self>() + self.map.capacity() * (size_of::<u64>() + size_of::<u32>() + 1)
    }

    fn fresh(rows: &[FsEntry]) -> Self {
        Self {
            seed: RandomState::new(),
            map: HashMap::with_capacity(rows.len()),
            #[cfg(test)]
            constant: false,
        }
    }

    fn from_parts(rows: &[FsEntry], mut lookup: Self) -> Option<Self> {
        for (at, row) in rows.iter().enumerate() {
            let at = u32::try_from(at).ok()?;
            let key = lookup.key(&row.name);
            if lookup.map.insert(key, at).is_some() {
                return None;
            }
        }
        Some(lookup)
    }

    fn key(&self, name: &ProviderPath) -> u64 {
        #[cfg(test)]
        if self.constant {
            return 0;
        }
        self.seed.hash_one(name)
    }

    fn patch(self, base: &[FsEntry], changes: &[Change]) -> Option<(Arc<[FsEntry]>, Self)> {
        if self.all_replacements(base, changes) {
            self.replace_in_place(base, changes)
        } else {
            self.rebuild(base, changes)
        }
    }

    // Rows stay at their indices, so no renumbering and no working copy of the array.
    fn all_replacements(&self, base: &[FsEntry], changes: &[Change]) -> bool {
        let mut vacated: Vec<&ProviderPath> = Vec::new();
        changes
            .iter()
            .all(|change| self.replaces(change, base, &mut vacated))
    }

    fn replaces<'a>(
        &self,
        change: &'a Change,
        base: &[FsEntry],
        vacated: &mut Vec<&'a ProviderPath>,
    ) -> bool {
        match change {
            Change::Added(entry) | Change::Modified(entry) => {
                !held(&entry.name, vacated) && self.find(base, &entry.name).is_some()
            }
            Change::Removed(_) => false,
            Change::Renamed { from, to } => {
                let ok = !held(from, vacated)
                    && !held(&to.name, vacated)
                    && self.find(base, from).is_some()
                    && (*from == to.name || self.find(base, &to.name).is_none());
                if ok && *from != to.name {
                    vacated.push(from);
                }
                ok
            }
        }
    }

    fn replace_in_place(
        mut self,
        base: &[FsEntry],
        changes: &[Change],
    ) -> Option<(Arc<[FsEntry]>, Self)> {
        let mut replacements: HashMap<u32, FsEntry> = HashMap::new();
        for change in changes {
            self.replace_one(base, change, &mut replacements)?;
        }
        let rows: Arc<[FsEntry]> = base
            .iter()
            .enumerate()
            .map(|(at, row)| {
                replacements
                    .remove(&(at as u32))
                    .unwrap_or_else(|| row.clone())
            })
            .collect();
        Some((rows, self))
    }

    fn replace_one(
        &mut self,
        base: &[FsEntry],
        change: &Change,
        replacements: &mut HashMap<u32, FsEntry>,
    ) -> Option<()> {
        match change {
            Change::Added(entry) | Change::Modified(entry) => {
                let at = self.find(base, &entry.name)?;
                replacements.insert(at, entry.clone());
            }
            Change::Renamed { from, to } => {
                let at = self.rename_one(base, from, to)?;
                replacements.insert(at, to.clone());
            }
            Change::Removed(_) => return None,
        }
        Some(())
    }

    fn rename_one(&mut self, base: &[FsEntry], from: &ProviderPath, to: &FsEntry) -> Option<u32> {
        let at = self.find(base, from)?;
        if *from != to.name {
            self.move_name(from, &to.name, at)?;
        }
        Some(at)
    }

    fn rebuild(mut self, base: &[FsEntry], changes: &[Change]) -> Option<(Arc<[FsEntry]>, Self)> {
        let mut rows: Vec<Option<FsEntry>> = base.iter().cloned().map(Some).collect();
        for change in changes {
            match change {
                Change::Added(entry) | Change::Modified(entry) => {
                    self.upsert(&mut rows, entry.clone())?;
                }
                Change::Removed(name) => self.remove(&mut rows, name),
                Change::Renamed { from, to } => self.rename(&mut rows, from, to.clone())?,
            }
        }
        let dropped = rows.iter().any(Option::is_none);
        let renumber = if dropped {
            Some(Vec::with_capacity(rows.len()))
        } else {
            None
        };
        let mut compact = Vec::with_capacity(rows.len());
        if let Some(mut renumber) = renumber {
            for row in rows {
                renumber.push(next_at(compact.len(), row.is_some())?);
                compact.extend(row);
            }
            self.renumber(&renumber)?;
            self.map.shrink_to_fit();
        } else {
            compact.extend(rows.into_iter().flatten());
        }
        let rows: Arc<[FsEntry]> = compact.into();
        Some((rows, self))
    }

    fn move_name(&mut self, from: &ProviderPath, to: &ProviderPath, at: u32) -> Option<()> {
        let from_key = self.key(from);
        let to_key = self.key(to);
        if self
            .map
            .get(&to_key)
            .is_some_and(|&existing| existing != at)
        {
            return None;
        }
        self.map.remove(&from_key);
        self.map.insert(to_key, at);
        Some(())
    }

    fn renumber(&mut self, renumber: &[u32]) -> Option<()> {
        for at in self.map.values_mut() {
            *at = *renumber.get(usize::try_from(*at).ok()?)?;
            if *at == u32::MAX {
                return None;
            }
        }
        Some(())
    }

    fn upsert(&mut self, rows: &mut Vec<Option<FsEntry>>, entry: FsEntry) -> Option<()> {
        let key = self.key(&entry.name);
        match self.map.get(&key).copied() {
            Some(at) => {
                let index = usize::try_from(at).ok()?;
                let row = rows.get(index)?.as_ref()?;
                if row.name != entry.name {
                    return None;
                }
                rows[index] = Some(entry);
                Some(())
            }
            None => self.push(rows, entry),
        }
    }

    fn push(&mut self, rows: &mut Vec<Option<FsEntry>>, entry: FsEntry) -> Option<()> {
        let key = self.key(&entry.name);
        if self.map.contains_key(&key) {
            return None;
        }
        let at = u32::try_from(rows.len()).ok()?;
        self.map.insert(key, at);
        rows.push(Some(entry));
        Some(())
    }

    fn remove(&mut self, rows: &mut [Option<FsEntry>], name: &ProviderPath) {
        let key = self.key(name);
        let Some(at) = self.map.get(&key).copied() else {
            return;
        };
        let Some(index) = usize::try_from(at).ok() else {
            return;
        };
        let matches = rows
            .get(index)
            .and_then(|row| row.as_ref())
            .is_some_and(|row| row.name == *name);
        if matches {
            self.map.remove(&key);
            rows[index] = None;
        }
    }

    fn rename(
        &mut self,
        rows: &mut Vec<Option<FsEntry>>,
        from: &ProviderPath,
        to: FsEntry,
    ) -> Option<()> {
        if *from != to.name {
            self.remove(rows, &to.name);
        }
        let from_key = self.key(from);
        let Some(at) = self.map.remove(&from_key) else {
            return self.push(rows, to);
        };
        let index = usize::try_from(at).ok()?;
        let row = rows.get(index)?.as_ref()?;
        if row.name != *from {
            return None;
        }
        let to_key = self.key(&to.name);
        if self.map.contains_key(&to_key) {
            return None;
        }
        self.map.insert(to_key, at);
        rows[index] = Some(to);
        Some(())
    }

    #[cfg(test)]
    pub(crate) fn colliding(rows: &[FsEntry]) -> Option<Self> {
        let mut lookup = Self::fresh(rows);
        lookup.constant = true;
        Self::from_parts(rows, lookup)
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.map.capacity()
    }

    #[cfg(test)]
    pub(crate) fn key_of(&self, name: &ProviderPath) -> u64 {
        self.key(name)
    }
}

// Row size is the entry struct plus its name bytes; heap slack and allocator headers are ignored.
fn row_bytes(row: &FsEntry) -> usize {
    size_of::<FsEntry>() + row.name.display().len()
}

struct IndexedRows {
    rows: Vec<Option<FsEntry>>,
    index: HashMap<ProviderPath, usize>,
}

impl IndexedRows {
    fn new(rows: &[FsEntry]) -> Self {
        let index = rows
            .iter()
            .enumerate()
            .map(|(at, row)| (row.name.clone(), at))
            .collect();
        Self {
            rows: rows.iter().cloned().map(Some).collect(),
            index,
        }
    }

    fn apply(&mut self, change: &Change) {
        match change {
            Change::Added(entry) | Change::Modified(entry) => self.upsert(entry.clone()),
            Change::Removed(name) => self.remove(name),
            Change::Renamed { from, to } => self.rename(from, to.clone()),
        }
    }

    fn upsert(&mut self, entry: FsEntry) {
        match self.index.get(&entry.name) {
            Some(&at) => self.rows[at] = Some(entry),
            None => self.push(entry),
        }
    }

    fn push(&mut self, entry: FsEntry) {
        self.index.insert(entry.name.clone(), self.rows.len());
        self.rows.push(Some(entry));
    }

    fn remove(&mut self, name: &ProviderPath) {
        if let Some(at) = self.index.remove(name) {
            self.rows[at] = None;
        }
    }

    fn rename(&mut self, from: &ProviderPath, to: FsEntry) {
        if *from != to.name {
            self.remove(&to.name);
        }
        match self.index.remove(from) {
            Some(at) => {
                self.index.insert(to.name.clone(), at);
                self.rows[at] = Some(to);
            }
            None => self.push(to),
        }
    }

    fn into_rows(self) -> Arc<[FsEntry]> {
        self.rows.into_iter().flatten().collect()
    }
}
