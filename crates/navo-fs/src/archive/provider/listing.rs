use navo_core::{FsError, Location, ProviderPath};

use crate::{Attributes, CancelToken, EntryBatch, FsEntry, ListSink};

use super::super::MAX_ENTRIES;
use super::diagnostics::ArchiveProblem;
use super::index::{self, Child};
use super::location::{self, ArchiveLocation};
use super::{ArchiveFs, errors};

pub(super) fn list(
    archive: &ArchiveFs,
    folder: &Location,
    sink: &mut dyn ListSink,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    let found = archive.parse(folder)?;
    let index = archive.load(&found.archive, folder, cancel)?;
    if cancel.is_cancelled() {
        return Err(errors::cancelled(folder));
    }
    check_folder(archive, &found, index.as_ref())?;
    let entries = index
        .children(&found.inner)
        .into_iter()
        .map(|child| child_entry(archive, &found, folder, child))
        .collect();
    if index.truncated {
        archive.record(folder, ArchiveProblem::Truncated { limit: MAX_ENTRIES });
    }
    if sink
        .batch(EntryBatch {
            entries,
            last: true,
        })
        .is_break()
    {
        return Ok(());
    }
    if cancel.is_cancelled() {
        return Err(errors::cancelled(folder));
    }
    Ok(())
}

fn check_folder(
    archive: &ArchiveFs,
    found: &ArchiveLocation,
    index: &index::ArchiveIndex,
) -> Result<(), FsError> {
    if found.inner.is_empty() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&found.inner);
    if let Some(reason) = index::unsafe_reason(&text) {
        archive.record(
            &found.location,
            ArchiveProblem::UnsafeName {
                name: text.into_owned(),
                reason,
            },
        );
        return Err(errors::unsupported(&found.location));
    }
    let Some((_, entry)) = index.lookup_with_index(&found.inner) else {
        if index.has_descendants(&found.inner) {
            return Ok(());
        }
        return Err(errors::not_found(&found.location));
    };
    if entry.kind.is_folder() || index.has_descendants(&found.inner) {
        return Ok(());
    }
    Err(errors::unsupported(&found.location))
}

fn child_entry(
    archive: &ArchiveFs,
    found: &ArchiveLocation,
    folder: &Location,
    child: Child,
) -> FsEntry {
    let display = String::from_utf8_lossy(&child.name).into_owned();
    let full = full_name(found, &display);
    let name = ProviderPath::from_bytes(&child.name);
    let row = row_location(found, &child, &display);
    if let Some(reason) = child.unsafe_reason {
        archive.record(
            row.as_ref().unwrap_or(folder),
            ArchiveProblem::UnsafeName {
                name: display.clone(),
                reason,
            },
        );
    }
    if child.encrypted {
        archive.record(
            row.as_ref().unwrap_or(folder),
            ArchiveProblem::Encrypted { name: full.clone() },
        );
    }
    if let Some(kind) = child.special {
        archive.record(
            row.as_ref().unwrap_or(folder),
            ArchiveProblem::Special {
                name: full.clone(),
                kind,
            },
        );
    }
    if child.duplicate {
        archive.record(
            row.as_ref().unwrap_or(folder),
            ArchiveProblem::Duplicate { name: full },
        );
    }
    FsEntry {
        name,
        kind: child.kind,
        size: child.size,
        modified_ms: child.modified_ms,
        attributes: readonly(),
    }
}

fn full_name(found: &ArchiveLocation, display: &str) -> String {
    if found.inner.is_empty() {
        display.to_owned()
    } else {
        format!("{}/{}", String::from_utf8_lossy(&found.inner), display)
    }
}

fn row_location(found: &ArchiveLocation, child: &Child, display: &str) -> Option<Location> {
    if child.unsafe_reason.is_some() {
        return location::raw_location(&found.archive, display).ok();
    }
    location::location(&found.archive, &full_name(found, display)).ok()
}

fn readonly() -> Attributes {
    Attributes {
        readonly: true,
        ..Attributes::default()
    }
}
