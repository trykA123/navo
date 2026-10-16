use std::cmp::Ordering;
use std::ops::ControlFlow;

use rusty_core::{Entry, FsError, Listing, Location};

use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSnapshot {
    pub listing: Listing,
    pub details: Vec<FsEntry>,
}

pub fn load_folder(
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
) -> Result<FolderSnapshot, FsError> {
    load_folder_reporting(provider, folder, cancel, &mut || {})
}

pub fn load_folder_reporting(
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
    on_batch: &mut dyn FnMut(),
) -> Result<FolderSnapshot, FsError> {
    let mut details = Vec::new();
    let mut sink = |batch: EntryBatch| {
        on_batch();
        details.extend(batch.entries);
        ControlFlow::Continue(())
    };
    provider.list(folder, &mut sink, cancel)?;
    let mut keyed: Vec<(String, FsEntry)> = details
        .into_iter()
        .map(|entry| (entry.name.display().to_lowercase(), entry))
        .collect();
    keyed.sort_by(|(a_key, a), (b_key, b)| order(a, b).then_with(|| a_key.cmp(b_key)));
    let details: Vec<FsEntry> = keyed.into_iter().map(|(_, entry)| entry).collect();
    let listing = Listing::new(
        details
            .iter()
            .map(|entry| Entry {
                name: entry.name.to_text(),
                is_folder: entry.kind == EntryKind::Folder,
            })
            .collect(),
    );
    Ok(FolderSnapshot { listing, details })
}

fn order(a: &FsEntry, b: &FsEntry) -> Ordering {
    let folder = |entry: &FsEntry| entry.kind != EntryKind::Folder;
    folder(a).cmp(&folder(b))
}

#[cfg(test)]
mod tests;
