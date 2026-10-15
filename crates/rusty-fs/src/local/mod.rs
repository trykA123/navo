#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;
#[cfg(any(windows, test))]
mod verbatim;

use std::ffi::OsStr;
use std::fs::{self, DirEntry, Metadata, ReadDir};
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};

use crate::{CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider};

const DEFAULT_BATCH: usize = 512;

#[derive(Debug, Clone)]
pub struct LocalFs {
    batch_size: usize,
}

impl LocalFs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            batch_size: DEFAULT_BATCH,
        }
    }

    #[must_use]
    pub fn with_batch_size(self, size: usize) -> Self {
        Self {
            batch_size: size.max(1),
        }
    }

    fn read_batch(
        &self,
        children: &mut Peekable<ReadDir>,
        folder: &Location,
    ) -> Result<Vec<FsEntry>, FsError> {
        let mut entries = Vec::with_capacity(self.batch_size.min(DEFAULT_BATCH));
        while entries.len() < self.batch_size {
            let Some(child) = children.next() else {
                break;
            };
            let child = child.map_err(|e| io_error(&e, folder))?;
            let name = child.file_name();
            match child.metadata() {
                Ok(metadata) => entries.push(entry(platform::entry_name(&name), &name, &metadata)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(_) => entries.extend(bare_entry(&child, &name)),
            }
        }
        Ok(entries)
    }
}

impl Default for LocalFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for LocalFs {
    fn id(&self) -> ProviderId {
        ProviderId::local()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let path = os_path(folder)?;
        let mut children = open_dir(&path, folder)?.peekable();
        loop {
            let entries = self.read_batch(&mut children, folder)?;
            let last = children.peek().is_none();
            if cancel.is_cancelled() {
                return Err(error(FsErrorKind::Cancelled, folder));
            }
            if sink.batch(EntryBatch { entries, last }).is_break() || last {
                return Ok(());
            }
        }
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        let path = os_path(location)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&e, location))?;
        let entry = match path.file_name() {
            Some(name) => entry(platform::entry_name(name), name, &metadata),
            None => entry(location.path().clone(), path.as_os_str(), &metadata),
        };
        Ok(entry)
    }
}

pub(crate) fn os_path(location: &Location) -> Result<PathBuf, FsError> {
    if !location.is_local() {
        return Err(error(FsErrorKind::NameInvalid, location));
    }
    platform::os_path(location.path()).ok_or_else(|| error(FsErrorKind::NameInvalid, location))
}

fn open_dir(path: &Path, folder: &Location) -> Result<ReadDir, FsError> {
    fs::read_dir(path).map_err(|e| {
        let mut failure = io_error(&e, folder);
        if failure.kind == FsErrorKind::NotFound
            && fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
        {
            failure.kind = FsErrorKind::NameInvalid;
        }
        failure
    })
}

fn entry(name: ProviderPath, os_name: &OsStr, metadata: &Metadata) -> FsEntry {
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        EntryKind::Link
    } else if file_type.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::File
    };
    FsEntry {
        name,
        kind,
        size: if kind == EntryKind::File {
            metadata.len()
        } else {
            0
        },
        modified_ms: modified_ms(metadata),
        attributes: platform::attributes(os_name, metadata),
    }
}

fn bare_entry(child: &DirEntry, name: &OsStr) -> Option<FsEntry> {
    let file_type = child.file_type().ok()?;
    let kind = if file_type.is_symlink() {
        EntryKind::Link
    } else if file_type.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::File
    };
    Some(FsEntry {
        name: platform::entry_name(name),
        kind,
        size: 0,
        modified_ms: None,
        attributes: platform::name_attributes(name),
    })
}

fn modified_ms(metadata: &Metadata) -> Option<i64> {
    match metadata.modified().ok()?.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis())
            .ok()
            .map(i64::wrapping_neg),
    }
}

fn error(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        kind,
        location: Some(location.clone()),
        os_code: None,
    }
}

fn io_error(failure: &io::Error, location: &Location) -> FsError {
    let os_code = failure.raw_os_error();
    let kind = os_code.map_or_else(|| kind_of_io(failure.kind()), platform::error_kind);
    FsError {
        kind,
        location: Some(location.clone()),
        os_code,
    }
}

fn kind_of_io(kind: io::ErrorKind) -> FsErrorKind {
    match kind {
        io::ErrorKind::NotFound => FsErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => FsErrorKind::AccessDenied,
        io::ErrorKind::NotADirectory | io::ErrorKind::InvalidFilename => FsErrorKind::NameInvalid,
        _ => FsErrorKind::Unknown,
    }
}

#[cfg(test)]
mod tests;
