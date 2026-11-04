use std::io::{Cursor, Read};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navo_core::{FsError, Location, ProviderId, ProviderPath};

use crate::{
    Attributes, CancelToken, EntryKind, FileStamp, FsEntry, SourceContent, SourceIdentity,
    SourceMetadata, SourceOps, TempCreated,
};

use super::super::stream;
use super::super::{ArchiveEntry, ArchiveKind, EntryAt, Stamp, unix_ms};
use super::diagnostics::ArchiveProblem;
use super::index::{self, ArchiveIndex};
use super::location::ArchiveLocation;
use super::{ArchiveFs, errors};

pub(super) fn metadata(archive: &ArchiveFs, location: &Location) -> Result<FsEntry, FsError> {
    metadata_with_cancel(archive, location, &CancelToken::new())
}

fn metadata_with_cancel(
    archive: &ArchiveFs,
    location: &Location,
    cancel: &CancelToken,
) -> Result<FsEntry, FsError> {
    let found = archive.parse(location)?;
    let index = archive.load(&found.archive, location, cancel)?;
    if cancel.is_cancelled() {
        return Err(errors::cancelled(location));
    }
    metadata_in(archive, &found, &index, location)
}

fn metadata_in(
    archive: &ArchiveFs,
    found: &ArchiveLocation,
    index: &ArchiveIndex,
    location: &Location,
) -> Result<FsEntry, FsError> {
    if found.inner.is_empty() {
        return Ok(folder_row(root_name(&found.archive)));
    }
    let text = String::from_utf8_lossy(&found.inner);
    if let Some(reason) = index::unsafe_reason(&text) {
        if let Some(entry) = index.lookup_unsafe(&text) {
            return Ok(entry_row(entry, &found.inner));
        }
        archive.record(
            location,
            ArchiveProblem::UnsafeName {
                name: text.into_owned(),
                reason,
            },
        );
        return Err(errors::name_invalid(location));
    }
    if let Some(entry) = index.lookup(&found.inner) {
        if entry.kind.is_folder() || index.has_descendants(&found.inner) {
            return Ok(folder_row(name_of(&found.inner)));
        }
        return Ok(entry_row(entry, &found.inner));
    }
    if index.has_descendants(&found.inner) {
        return Ok(folder_row(name_of(&found.inner)));
    }
    Err(errors::not_found(location))
}

pub(super) fn open(
    archive: &ArchiveFs,
    location: &Location,
) -> Result<Box<dyn Read + Send>, FsError> {
    open_with_cancel(archive, location, &CancelToken::new())
}

fn open_with_cancel(
    archive: &ArchiveFs,
    location: &Location,
    cancel: &CancelToken,
) -> Result<Box<dyn Read + Send>, FsError> {
    let found = archive.parse(location)?;
    let index = archive.load(&found.archive, location, cancel)?;
    if cancel.is_cancelled() {
        return Err(errors::cancelled(location));
    }
    let (entry_index, entry) = read_entry(archive, &found, &index, location)?;
    if entry.size == 0 {
        return Ok(Box::new(Cursor::new(Vec::new())));
    }
    Ok(Box::new(stream::open(
        stream::ReadRequest {
            path: found.archive,
            at: EntryAt {
                index: entry_index,
                limit: u64::MAX,
            },
            header_offset: entry.header_offset,
            zip: index.zip(),
        },
        cancel.clone(),
    )))
}

fn read_entry<'a>(
    archive: &ArchiveFs,
    found: &ArchiveLocation,
    index: &'a ArchiveIndex,
    location: &Location,
) -> Result<(usize, &'a ArchiveEntry), FsError> {
    if found.inner.is_empty() {
        return Err(errors::unsupported(location));
    }
    let text = String::from_utf8_lossy(&found.inner);
    if let Some(reason) = index::unsafe_reason(&text) {
        archive.record(
            location,
            ArchiveProblem::UnsafeName {
                name: text.into_owned(),
                reason,
            },
        );
        return Err(errors::name_invalid(location));
    }
    let Some((entry_index, entry)) = index.lookup_with_index(&found.inner) else {
        if index.has_descendants(&found.inner) {
            return Err(errors::unsupported(location));
        }
        return Err(errors::not_found(location));
    };
    if entry.kind.is_folder() || index.has_descendants(&found.inner) {
        return Err(errors::unsupported(location));
    }
    if entry.kind.is_special() {
        archive.record(
            location,
            ArchiveProblem::Special {
                name: display(&found.inner),
                kind: entry.kind,
            },
        );
        return Err(errors::unsupported(location));
    }
    if entry.is_encrypted {
        archive.record(
            location,
            ArchiveProblem::Encrypted {
                name: display(&found.inner),
            },
        );
        return Err(errors::unsupported(location));
    }
    Ok((entry_index, entry))
}

fn source_metadata(
    archive: &ArchiveFs,
    location: &Location,
    cancel: &CancelToken,
) -> Result<SourceMetadata, FsError> {
    let found = archive.parse(location)?;
    let index = archive.load(&found.archive, location, cancel)?;
    if cancel.is_cancelled() {
        return Err(errors::cancelled(location));
    }
    let (_, entry) = read_entry(archive, &found, &index, location)?;
    Ok(SourceMetadata {
        kind: EntryKind::File,
        identity: SourceIdentity::Stamp(FileStamp {
            size: entry.size,
            modified_ms: state_ms(&index),
        }),
        modified: system_time(entry.modified),
    })
}

impl SourceOps for ArchiveFs {
    fn id(&self) -> ProviderId {
        crate::Provider::id(self)
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        source_metadata(self, location, &CancelToken::new())
    }

    fn metadata_with_cancel(
        &self,
        location: &Location,
        cancel: &CancelToken,
    ) -> Result<SourceMetadata, FsError> {
        source_metadata(self, location, cancel)
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        open(self, location)
    }

    fn open_into(
        &self,
        location: &Location,
        (_temp, _created): (&Location, TempCreated<'_>),
        cancel: &CancelToken,
    ) -> Result<SourceContent, FsError> {
        open_with_cancel(self, location, cancel).map(SourceContent::Stream)
    }
}

fn entry_row(entry: &ArchiveEntry, inner: &[u8]) -> FsEntry {
    FsEntry {
        name: name_of(inner),
        kind: match entry.kind {
            ArchiveKind::Folder => EntryKind::Folder,
            ArchiveKind::Symlink | ArchiveKind::Hardlink => EntryKind::Link,
            _ => EntryKind::File,
        },
        size: if entry.kind.is_folder() {
            0
        } else {
            entry.size
        },
        modified_ms: unix_ms(entry.modified),
        attributes: readonly(),
    }
}

fn folder_row(name: ProviderPath) -> FsEntry {
    FsEntry {
        name,
        kind: EntryKind::Folder,
        size: 0,
        modified_ms: None,
        attributes: readonly(),
    }
}

fn root_name(archive: &Path) -> ProviderPath {
    archive.file_name().map_or_else(
        || ProviderPath::from_text("/"),
        |name| ProviderPath::from_text(&name.to_string_lossy()),
    )
}

fn name_of(inner: &[u8]) -> ProviderPath {
    let name = inner.rsplit(|byte| *byte == b'/').next().unwrap_or(inner);
    ProviderPath::from_bytes(name)
}

fn display(inner: &[u8]) -> String {
    String::from_utf8_lossy(inner).into_owned()
}

fn readonly() -> Attributes {
    Attributes {
        readonly: true,
        ..Attributes::default()
    }
}

fn state_ms(index: &ArchiveIndex) -> Option<i64> {
    index
        .state
        .modified?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
}

fn system_time(stamp: Option<Stamp>) -> Option<SystemTime> {
    let millis = match stamp? {
        Stamp::UnixMs(millis) => millis,
        Stamp::Local(_) => return None,
    };
    if millis >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_millis(u64::try_from(millis).ok()?))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_millis(millis.unsigned_abs()))
    }
}
