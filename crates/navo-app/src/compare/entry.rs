//! Conversions shared by the local and Git sides.

use std::ops::ControlFlow;

use navo_compare::model::{CancelToken, EntryKind, Reason, RelPath, SideEntry};
use navo_core::{FsErrorKind, ProviderPath};
use navo_fs::{EntryBatch, EntryKind as FsKind, FsEntry};

pub(super) fn reason_of(kind: FsErrorKind) -> Reason {
    match kind {
        FsErrorKind::NotFound => Reason::NotFound,
        FsErrorKind::AccessDenied | FsErrorKind::SharingViolation | FsErrorKind::Locked => {
            Reason::AccessDenied
        }
        FsErrorKind::Cancelled => Reason::Stopped,
        FsErrorKind::NotSupportedYet | FsErrorKind::SourceFilterFailed => Reason::Unsupported,
        FsErrorKind::ChangedOnDisk => Reason::ChangedOnDisk,
        _ => Reason::ReadError,
    }
}

/// Joins a folder row path and a child name with `/`.
pub(super) fn child_rel(parent: &RelPath, name: &ProviderPath) -> RelPath {
    let parent = parent.as_path().to_text();
    let name = name.to_text();
    let text = if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    };
    RelPath::new(ProviderPath::from_text(&text))
}

pub(super) fn side_entry(parent: &RelPath, entry: FsEntry) -> SideEntry {
    let kind = match entry.kind {
        FsKind::File => EntryKind::File,
        FsKind::Folder => EntryKind::Folder,
        FsKind::Link => EntryKind::Link,
    };
    SideEntry::new(
        child_rel(parent, &entry.name),
        kind,
        entry.size,
        entry.modified_ms,
    )
}

/// The path segments of a row path, without empty parts.
pub(super) fn segments(rel: &RelPath) -> Vec<String> {
    rel.as_path()
        .to_text()
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Feeds one listing batch to the engine's sink; breaks on cancel or when the sink stops.
pub(super) fn feed(
    folder: &RelPath,
    batch: EntryBatch,
    sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
    cancel: &CancelToken,
) -> ControlFlow<()> {
    for found in batch.entries {
        if cancel.is_cancelled() {
            return ControlFlow::Break(());
        }
        sink(side_entry(folder, found))?;
    }
    ControlFlow::Continue(())
}
