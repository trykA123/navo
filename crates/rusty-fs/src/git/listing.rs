use std::collections::HashSet;

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};
use rusty_git::{TreeEntry, TreeKind};

use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, ListSink};

use super::{GitFs, errors, reading, tree};

pub(super) fn list(
    git: &GitFs,
    folder: &Location,
    sink: &mut dyn ListSink,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    let found = git.parse(folder)?;
    let entries = git.tree(&found)?;
    if entries.is_empty() && !found.inner.is_empty() {
        return Err(errors::not_found(folder));
    }
    if !found.inner.is_empty() {
        if tree::find(&entries, &found.inner).is_some() {
            return Err(errors::unsupported(folder));
        }
        if !tree::has_descendants(&entries, &found.inner) {
            return Err(errors::not_found(folder));
        }
    }
    if cancel.is_cancelled() {
        return Err(errors::at(FsErrorKind::Cancelled, folder));
    }
    let mut prefix = found.inner.clone();
    if !prefix.is_empty() {
        prefix.push(b'/');
    }
    let children = children(&entries, &prefix);
    let _ = sink.batch(EntryBatch {
        entries: children,
        last: true,
    });
    Ok(())
}

fn children(entries: &[TreeEntry], prefix: &[u8]) -> Vec<FsEntry> {
    let mut children = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        let Some(rest) = entry.path.as_bytes().strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        if let Some(slash) = rest.iter().position(|byte| *byte == b'/') {
            let name = &rest[..slash];
            if seen.insert(name.to_vec()) {
                children.push(folder_child(name));
            }
        } else {
            children.push(leaf_child(rest, entry));
        }
    }
    children
}

fn folder_child(name: &[u8]) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_bytes(name),
        kind: EntryKind::Folder,
        size: 0,
        modified_ms: None,
        attributes: reading::readonly(),
    }
}

fn leaf_child(name: &[u8], entry: &TreeEntry) -> FsEntry {
    let kind = match entry.kind {
        TreeKind::Link => EntryKind::Link,
        TreeKind::File => EntryKind::File,
        TreeKind::Folder | TreeKind::Submodule => EntryKind::Folder,
    };
    FsEntry {
        name: ProviderPath::from_bytes(name),
        kind,
        size: entry.size.unwrap_or(0),
        modified_ms: None,
        attributes: reading::readonly(),
    }
}
