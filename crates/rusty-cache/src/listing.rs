use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use rusty_core::ProviderPath;
use rusty_fs::FsEntry;

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

    pub(crate) fn patched(&self, changes: &[Change], changed_at: SystemTime) -> Self {
        let mut rows = IndexedRows::new(&self.rows);
        for change in changes {
            rows.apply(change);
        }
        Self {
            rows: rows.into_rows(),
            read_at: changed_at.max(self.read_at),
            ..self.clone()
        }
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
