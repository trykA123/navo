use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use navo_core::{FsError, FsErrorKind, Location, ProviderId};
use navo_fs::{
    CancelToken, EntryKind, SourceContent, SourceIdentity, SourceMetadata, SourceOps, TempCreated,
    os_path,
};
use navo_shell::{Cancel, VirtualClipboard, VirtualContent, VirtualError, VirtualReader};

use super::items::FileEntry;
use super::mark::{self, Mark};

/// The text of the failure that ended the job, kept for the job's status.
pub(super) type LastError = Arc<Mutex<Option<String>>>;

/// Reads pasted virtual files for a copy job. Items are served one at a time, so the job runs
/// with one worker.
pub(super) struct VirtualFiles {
    pub provider: ProviderId,
    pub clipboard: VirtualClipboard,
    pub files: HashMap<Location, FileEntry>,
    pub mark: Mark,
    pub last_error: LastError,
}

impl VirtualFiles {
    fn file(&self, location: &Location) -> Result<FileEntry, FsError> {
        self.files
            .get(location)
            .copied()
            .ok_or_else(|| failure(FsErrorKind::NotFound, location))
    }

    fn fail(&self, location: &Location, error: &VirtualError) -> FsError {
        let kind = match error {
            VirtualError::Cancelled => return failure(FsErrorKind::Cancelled, location),
            VirtualError::Stalled => FsErrorKind::Timeout,
            VirtualError::Malformed | VirtualError::Source(_) => FsErrorKind::Unknown,
        };
        record(&self.last_error, explain(&name_of(location), error));
        failure(kind, location)
    }

    fn store(
        &self,
        (location, file): (&Location, FileEntry),
        (temp, created): (&Location, TempCreated<'_>),
        cancel: &Cancel,
    ) -> Result<(), FsError> {
        let path = os_path(temp)?;
        let mut hook_failed = None;
        let mut announce = || {
            created().map_err(|error| {
                let reason = String::from("the job journal refused the temporary file");
                hook_failed = Some(error);
                reason
            })
        };
        let written = self
            .clipboard
            .write_storage(file.index, (&path, &mut announce), cancel);
        if let Some(error) = hook_failed {
            return Err(error);
        }
        written.map_err(|error| self.fail(location, &error))?;
        finish_storage((&path, &name_of(location)), &self.mark, file).map_err(|reason| {
            let _ = std::fs::remove_file(&path);
            record(&self.last_error, reason);
            failure(FsErrorKind::Unknown, location)
        })
    }
}

impl SourceOps for VirtualFiles {
    fn id(&self) -> ProviderId {
        self.provider.clone()
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        let file = self.file(location)?;
        Ok(SourceMetadata {
            kind: EntryKind::File,
            identity: SourceIdentity::Advisory,
            modified: file.modified,
        })
    }

    /// Unused: a virtual item is read once, through [`SourceOps::open_into`].
    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        Err(failure(FsErrorKind::NotSupportedYet, location))
    }

    fn open_into(
        &self,
        location: &Location,
        (temp, created): (&Location, TempCreated<'_>),
        cancel: &CancelToken,
    ) -> Result<SourceContent, FsError> {
        let file = self.file(location)?;
        let token = cancel.clone();
        let cancel: Cancel = Arc::new(move || token.is_cancelled());
        match self.clipboard.open(file.index, Arc::clone(&cancel)) {
            Err(error) => Err(self.fail(location, &error)),
            Ok(VirtualContent::Storage) => {
                self.store((location, file), (temp, created), &cancel)?;
                Ok(SourceContent::Written)
            }
            Ok(VirtualContent::Stream(reader)) => Ok(SourceContent::Stream(Box::new(Checked {
                reader,
                read: 0,
                size: file.size,
                ended: false,
                mark: self.mark.clone(),
                temp: temp.clone(),
                name: name_of(location),
                last_error: Arc::clone(&self.last_error),
            }))),
        }
    }
}

/// One item's bytes. At the end it refuses a short item and writes the mark, before the job
/// flushes the file and renames it into place.
struct Checked {
    reader: VirtualReader,
    read: u64,
    size: Option<u64>,
    ended: bool,
    mark: Mark,
    temp: Location,
    name: String,
    last_error: LastError,
}

impl Checked {
    fn end(&mut self) -> io::Result<()> {
        self.ended = true;
        if self.size.is_some_and(|size| self.read < size) {
            let reason = navo_l10n::t!("paste-virtual-short", name = self.name.clone());
            return Err(self.recorded(reason));
        }
        let Mark::Zone(zone) = &self.mark else {
            return Ok(());
        };
        let written = os_path(&self.temp)
            .map_err(|_| io::Error::other("the file path is not valid"))
            .and_then(|path| mark::write(&path, zone));
        written.map_err(|error| {
            let reason = mark_failed(&self.name, &error);
            self.recorded(reason)
        })
    }

    fn recorded(&self, reason: String) -> io::Error {
        record(&self.last_error, reason.clone());
        io::Error::other(reason)
    }
}

impl Read for Checked {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self
            .reader
            .read(buf)
            .inspect_err(|error| match VirtualError::of(error) {
                Some(VirtualError::Cancelled) | None => {}
                Some(virtual_error) => record(&self.last_error, explain(&self.name, virtual_error)),
            })?;
        self.read += count as u64;
        if count == 0 && !buf.is_empty() && !self.ended {
            self.end()?;
        }
        Ok(count)
    }
}

/// Marks, dates and flushes a mail the source saved itself.
fn finish_storage((path, name): (&Path, &str), mark: &Mark, file: FileEntry) -> Result<(), String> {
    if let Mark::Zone(zone) = mark {
        mark::write(path, zone).map_err(|error| mark_failed(name, &error))?;
    }
    let written = std::fs::OpenOptions::new().write(true).open(path);
    written
        .and_then(|output| {
            if let Some(modified) = file.modified {
                output.set_modified(modified)?;
            }
            output.sync_all()
        })
        .map_err(|error| {
            navo_l10n::t!(
                "paste-virtual-failed",
                name = name.to_owned(),
                error = error.to_string()
            )
        })
}

fn mark_failed(name: &str, error: &io::Error) -> String {
    navo_l10n::t!(
        "paste-virtual-mark-failed",
        name = name.to_owned(),
        error = error.to_string()
    )
}

fn record(last_error: &LastError, reason: String) {
    *last_error.lock().unwrap_or_else(PoisonError::into_inner) = Some(reason);
}

/// The text a job shows when reading one item failed.
pub(super) fn explain(name: &str, error: &VirtualError) -> String {
    let name = name.to_owned();
    match error {
        VirtualError::Stalled => navo_l10n::t!("paste-virtual-stalled", name = name),
        VirtualError::Malformed => navo_l10n::tr("paste-virtual-damaged"),
        VirtualError::Cancelled | VirtualError::Source(_) => navo_l10n::t!(
            "paste-virtual-failed",
            name = name,
            error = error.to_string()
        ),
    }
}

fn name_of(location: &Location) -> String {
    location.path().display().trim_start_matches('/').to_owned()
}

fn failure(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}
