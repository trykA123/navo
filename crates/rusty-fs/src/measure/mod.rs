use std::ops::ControlFlow;

use rusty_core::{FsError, FsErrorKind, Location};

use crate::{CancelToken, EntryBatch, EntryKind, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FolderSize {
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    pub skipped: u64,
}

pub fn folder_size(
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
) -> Result<FolderSize, FsError> {
    let mut size = FolderSize::default();
    let mut pending = vec![folder.clone()];
    let mut top = true;
    while let Some(location) = pending.pop() {
        match list_one(provider, &location, cancel, &mut size) {
            Ok(children) => pending.extend(children),
            Err(error) if top || error.kind == FsErrorKind::Cancelled => return Err(error),
            Err(_) => size.skipped += 1,
        }
        top = false;
    }
    Ok(size)
}

fn list_one(
    provider: &dyn Provider,
    location: &Location,
    cancel: &CancelToken,
    size: &mut FolderSize,
) -> Result<Vec<Location>, FsError> {
    let mut children = Vec::new();
    let mut sink = |batch: EntryBatch| {
        for entry in batch.entries {
            if entry.kind == EntryKind::Folder {
                size.folders += 1;
                children.extend(location.join(&entry.name).ok());
            } else {
                size.files += 1;
                size.bytes = size.bytes.saturating_add(entry.size);
            }
        }
        ControlFlow::Continue(())
    };
    provider.list(location, &mut sink, cancel)?;
    Ok(children)
}

#[cfg(test)]
mod tests;
