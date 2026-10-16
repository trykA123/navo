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

    pub(crate) fn patched(&self, changes: impl IntoIterator<Item = Change>) -> Self {
        let mut rows = self.rows.to_vec();
        changes
            .into_iter()
            .for_each(|change| apply(&mut rows, change));
        Self {
            rows: rows.into(),
            ..self.clone()
        }
    }
}

// Row size is the entry struct plus its name bytes; heap slack and allocator headers are ignored.
fn row_bytes(row: &FsEntry) -> usize {
    size_of::<FsEntry>() + row.name.display().len()
}

fn apply(rows: &mut Vec<FsEntry>, change: Change) {
    match change {
        Change::Added(entry) | Change::Modified(entry) => upsert(rows, entry),
        Change::Removed(name) => rows.retain(|row| row.name != name),
        Change::Renamed { from, to } => rename(rows, &from, to),
    }
}

fn upsert(rows: &mut Vec<FsEntry>, entry: FsEntry) {
    match rows.iter_mut().find(|row| row.name == entry.name) {
        Some(row) => *row = entry,
        None => rows.push(entry),
    }
}

fn rename(rows: &mut Vec<FsEntry>, from: &ProviderPath, to: FsEntry) {
    if *from != to.name {
        rows.retain(|row| row.name != to.name);
    }
    match rows.iter_mut().find(|row| row.name == *from) {
        Some(row) => *row = to,
        None => rows.push(to),
    }
}
