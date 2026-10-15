use std::ops::ControlFlow;

use rusty_core::{
    ExistingItem, FsError, FsErrorKind, ItemKind, Location, ProviderPath, ScannedItem,
};

use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, Provider, load_folder};

pub fn scan(
    provider: &dyn Provider,
    sources: &[Location],
    cancel: &CancelToken,
) -> Result<Vec<ScannedItem>, FsError> {
    let mut items = Vec::new();
    for source in sources {
        let entry = provider.metadata(source)?;
        let mut pending = vec![(source.clone(), entry, 0_u32)];
        while let Some((location, entry, depth)) = pending.pop() {
            if cancel.is_cancelled() {
                return Err(cancelled(&location));
            }
            if entry.kind == EntryKind::Folder {
                pending.extend(children(provider, &location, depth, cancel)?);
            }
            items.push(scanned(location, &entry, depth));
        }
    }
    Ok(items)
}

fn children(
    provider: &dyn Provider,
    folder: &Location,
    depth: u32,
    cancel: &CancelToken,
) -> Result<Vec<(Location, FsEntry, u32)>, FsError> {
    let details = load_folder(provider, folder, cancel)?.details;
    Ok(details
        .into_iter()
        .rev()
        .filter_map(|child| Some((folder.join(&child.name).ok()?, child, depth + 1)))
        .collect())
}

pub fn existing_at(
    provider: &dyn Provider,
    items: &[ScannedItem],
    target: &Location,
    cancel: &CancelToken,
) -> Result<Vec<ExistingItem>, FsError> {
    let mut existing = Vec::new();
    for folder in target_folders(items, target) {
        let mut sink = |batch: EntryBatch| {
            existing.extend(
                batch
                    .entries
                    .into_iter()
                    .filter_map(|entry| existing_item(&folder, entry)),
            );
            ControlFlow::Continue(())
        };
        match provider.list(&folder, &mut sink, cancel) {
            Ok(()) => {}
            Err(error)
                if matches!(error.kind, FsErrorKind::NotFound | FsErrorKind::NameInvalid) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(existing)
}

fn existing_item(folder: &Location, entry: FsEntry) -> Option<ExistingItem> {
    Some(ExistingItem {
        location: folder.join(&entry.name).ok()?,
        kind: item_kind(entry.kind),
        size: entry.size,
        modified_ms: entry.modified_ms,
    })
}

fn target_folders(items: &[ScannedItem], target: &Location) -> Vec<Location> {
    let mut folders = vec![target.clone()];
    let mut names: Vec<ProviderPath> = Vec::new();
    for item in items {
        names.truncate(item.depth as usize);
        let Some(name) = name_of(&item.location) else {
            continue;
        };
        names.push(name);
        if item.kind != ItemKind::Folder {
            continue;
        }
        let joined = names
            .iter()
            .try_fold(target.clone(), |folder, name| folder.join(name).ok());
        folders.extend(joined);
    }
    folders
}

fn name_of(location: &Location) -> Option<ProviderPath> {
    let parent = location.parent()?;
    let rest = location.as_str().strip_prefix(parent.as_str())?;
    let name = rest.trim_start_matches(['\\', '/']);
    (!name.is_empty()).then(|| ProviderPath::from_text(name))
}

fn scanned(location: Location, entry: &FsEntry, depth: u32) -> ScannedItem {
    let kind = item_kind(entry.kind);
    ScannedItem {
        size: if kind == ItemKind::File {
            entry.size
        } else {
            0
        },
        location,
        depth,
        kind,
        modified_ms: entry.modified_ms,
    }
}

fn item_kind(kind: EntryKind) -> ItemKind {
    match kind {
        EntryKind::File => ItemKind::File,
        EntryKind::Folder => ItemKind::Folder,
        EntryKind::Link => ItemKind::Link,
    }
}

fn cancelled(location: &Location) -> FsError {
    FsError {
        kind: FsErrorKind::Cancelled,
        location: Some(location.clone()),
        os_code: None,
    }
}

#[cfg(test)]
mod tests;
