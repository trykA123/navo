use std::io::Read;

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};
use rusty_git::{BlobReader, GitPath, TreeEntry, TreeKind};

use crate::{Attributes, EntryKind, FsEntry};

use super::GitFs;
use super::errors;
use super::location::GitLocation;
use super::tree;

pub(super) fn open_entry(
    git: &GitFs,
    location: &Location,
) -> Result<(GitLocation, TreeEntry), FsError> {
    let found = git.parse(location)?;
    let entries = git.tree(&found)?;
    let entry = tree::find(&entries, &found.inner)
        .cloned()
        .ok_or_else(|| errors::not_found(location))?;
    Ok((found, entry))
}

pub(super) fn link_target(git: &GitFs, at: &Location) -> Result<Option<Location>, FsError> {
    let (found, entry) = open_entry(git, at)?;
    if entry.kind != TreeKind::Link {
        return Ok(None);
    }
    let resolved = git.resolved(&found)?;
    let mut reader = resolved
        .source
        .read_blob(&entry.id)
        .map_err(|error| git.fail(at, error))?;
    let mut target = Vec::new();
    reader
        .read_to_end(&mut target)
        .map_err(|_| errors::at(FsErrorKind::Unknown, at))?;
    let Ok(target) = std::str::from_utf8(&target) else {
        return Ok(None);
    };
    let Some(inner) = resolve_target(&found.inner, target) else {
        return Ok(None);
    };
    Ok(super::location(&found.repo, &found.rev, &inner).ok())
}

fn resolve_target(link: &[u8], target: &str) -> Option<String> {
    if target.is_empty() || target.starts_with('/') {
        return None;
    }
    let parent = match link.iter().rposition(|byte| *byte == b'/') {
        Some(at) => &link[..at],
        None => &[],
    };
    let parent = std::str::from_utf8(parent).ok()?;
    let mut parts: Vec<&str> = parent.split('/').filter(|part| !part.is_empty()).collect();
    for part in target.split('/').filter(|part| !part.is_empty()) {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(parts.join("/"))
}

pub(super) fn metadata(git: &GitFs, location: &Location) -> Result<FsEntry, FsError> {
    let found = git.parse(location)?;
    let entries = git.tree(&found)?;
    if found.inner.is_empty() {
        return Ok(folder_entry(&found));
    }
    if let Some(entry) = tree::find(&entries, &found.inner) {
        return entry_metadata(entry, location);
    }
    if tree::has_descendants(&entries, &found.inner) {
        return Ok(folder_entry(&found));
    }
    Err(errors::not_found(location))
}

pub(super) fn read(git: &GitFs, location: &Location) -> Result<BlobReader, FsError> {
    let (found, entry) = open_entry(git, location)?;
    match entry.kind {
        TreeKind::File => {
            let resolved = git.resolved(&found)?;
            let path = GitPath::new(found.inner.clone());
            let blob = resolved
                .source
                .read_blob_worktree_form(&found.rev, &path)
                .map_err(|error| git.fail(location, error))?;
            if let Some(reason) = blob.reason {
                git.record_filter(location, reason);
            }
            Ok(blob.reader)
        }
        TreeKind::Link => git
            .resolved(&found)?
            .source
            .read_blob(&entry.id)
            .map_err(|error| git.fail(location, error)),
        TreeKind::Folder | TreeKind::Submodule => Err(errors::unsupported(location)),
    }
}

fn entry_metadata(entry: &TreeEntry, location: &Location) -> Result<FsEntry, FsError> {
    if entry.kind == TreeKind::Submodule {
        return Err(errors::unsupported(location));
    }
    let kind = match entry.kind {
        TreeKind::Link => EntryKind::Link,
        TreeKind::File => EntryKind::File,
        TreeKind::Folder | TreeKind::Submodule => EntryKind::Folder,
    };
    Ok(FsEntry {
        name: name_of(entry.path.as_bytes()),
        kind,
        size: entry.size.unwrap_or(0),
        modified_ms: None,
        attributes: readonly(),
    })
}

fn folder_entry(found: &GitLocation) -> FsEntry {
    let name = if found.inner.is_empty() {
        found
            .repo
            .file_name()
            .map(|name| ProviderPath::from_text(&name.to_string_lossy()))
            .unwrap_or_else(|| ProviderPath::from_text("/"))
    } else {
        name_of(&found.inner)
    };
    FsEntry {
        name,
        kind: EntryKind::Folder,
        size: 0,
        modified_ms: None,
        attributes: readonly(),
    }
}

pub(super) fn name_of(path: &[u8]) -> ProviderPath {
    let name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    ProviderPath::from_bytes(name)
}

pub(super) fn readonly() -> Attributes {
    Attributes {
        readonly: true,
        ..Attributes::default()
    }
}
