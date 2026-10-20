mod link;
#[cfg(target_os = "linux")]
mod linux_watch;
#[cfg(target_os = "linux")]
use linux_watch as platform_watch;
#[cfg(any(windows, test))]
mod notify_info;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;
mod ops;
#[cfg(any(windows, test))]
mod verbatim;
#[cfg(windows)]
mod win_copy;
#[cfg(windows)]
mod win_list;
#[cfg(windows)]
mod win_lock;
#[cfg(windows)]
mod win_replace;
#[cfg(windows)]
mod win_volume;
#[cfg(windows)]
mod win_watch;
#[cfg(windows)]
use win_watch as platform_watch;

use std::ffi::OsStr;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusty_core::{FsError, FsErrorKind, Location, LockHolder, ProviderId, ProviderPath};

pub use link::make_link;

#[cfg(any(windows, target_os = "linux"))]
use crate::watch::Depth;
use crate::{CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider};
#[cfg(any(windows, target_os = "linux"))]
use crate::{Watch, WatchSink};

const DEFAULT_BATCH: usize = 512;

type Children<'a> = Box<dyn Iterator<Item = Result<FsEntry, FsError>> + 'a>;

#[derive(Debug, Clone)]
pub struct AutoTuning {
    pub unbuffered_from: u64,
    pub ssd_workers: usize,
    pub network_workers: usize,
    pub slow_workers: usize,
}

impl Default for AutoTuning {
    fn default() -> Self {
        Self {
            unbuffered_from: 256 * 1024 * 1024,
            ssd_workers: 4,
            network_workers: 8,
            slow_workers: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalFs {
    batch_size: usize,
    #[cfg(windows)]
    auto_tuning: AutoTuning,
    #[cfg(windows)]
    std_listing: bool,
}

impl LocalFs {
    #[must_use]
    pub fn new() -> Self {
        Self::with_auto_tuning(AutoTuning::default())
    }

    #[must_use]
    #[cfg_attr(
        not(windows),
        expect(unused_variables, reason = "auto tuning is only used on Windows")
    )]
    pub fn with_auto_tuning(auto_tuning: AutoTuning) -> Self {
        Self {
            batch_size: DEFAULT_BATCH,
            #[cfg(windows)]
            auto_tuning,
            #[cfg(windows)]
            std_listing: false,
        }
    }

    #[must_use]
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }

    #[must_use]
    pub fn with_std_listing(self) -> Self {
        Self {
            #[cfg(windows)]
            std_listing: true,
            ..self
        }
    }

    fn children<'a>(&self, path: &Path, folder: &'a Location) -> Result<Children<'a>, FsError> {
        #[cfg(windows)]
        if !self.std_listing {
            let children = win_list::open(path).map_err(|e| open_failure(&e, path, folder))?;
            return Ok(Box::new(
                children.map(move |child| child.map_err(|e| io_error(&e, folder))),
            ));
        }
        let children = fs::read_dir(path).map_err(|e| open_failure(&e, path, folder))?;
        Ok(Box::new(
            children.filter_map(move |child| std_entry(child, folder)),
        ))
    }

    fn read_batch(&self, children: &mut Peekable<Children<'_>>) -> Result<Vec<FsEntry>, FsError> {
        children.by_ref().take(self.batch_size).collect()
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
        Capabilities {
            watch: cfg!(any(windows, target_os = "linux")),
            ..Capabilities::default()
        }
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let path = os_path(folder)?;
        let mut children = self.children(&path, folder)?.peekable();
        loop {
            let entries = self.read_batch(&mut children)?;
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
        Ok(followed(entry, &path))
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn watch(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        let path = os_path(folder)?;
        Watch::start(folder, sink, |raw| {
            platform_watch::start(&path, folder, raw, Depth::Folder)
                .map_err(|e| io_error(&e, folder))
        })
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        let path = os_path(folder)?;
        Watch::start_tree(folder, sink, |raw| {
            platform_watch::start(&path, folder, raw, Depth::Subtree)
                .map_err(|e| io_error(&e, folder))
        })
    }
}

pub fn os_path(location: &Location) -> Result<PathBuf, FsError> {
    if !location.is_local() {
        return Err(error(FsErrorKind::NameInvalid, location));
    }
    platform::os_path(location.path()).ok_or_else(|| error(FsErrorKind::NameInvalid, location))
}

fn open_failure(failure: &io::Error, path: &Path, folder: &Location) -> FsError {
    let mut failure = io_error(failure, folder);
    if failure.kind == FsErrorKind::NotFound
        && fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
    {
        failure.kind = FsErrorKind::NameInvalid;
    }
    failure
}

fn std_entry(child: io::Result<DirEntry>, folder: &Location) -> Option<Result<FsEntry, FsError>> {
    let child = match child {
        Ok(child) => child,
        Err(e) => return Some(Err(io_error(&e, folder))),
    };
    let name = child.file_name();
    let found = match child.metadata() {
        Ok(metadata) => entry(platform::entry_name(&name), &name, &metadata),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(_) => bare_entry(&child, &name)?,
    };
    Some(Ok(followed(found, &child.path())))
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

fn followed(mut entry: FsEntry, path: &Path) -> FsEntry {
    if entry.kind == EntryKind::Link {
        entry.attributes.link_to_folder = fs::metadata(path).is_ok_and(|m| m.is_dir());
    }
    entry
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
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

fn io_error(failure: &io::Error, location: &Location) -> FsError {
    let os_code = failure.raw_os_error();
    let kind = os_code.map_or_else(|| kind_of_io(failure.kind()), platform::error_kind);
    let holders = match kind {
        FsErrorKind::SharingViolation | FsErrorKind::Locked => lock_holders(location),
        _ => Vec::new(),
    };
    FsError {
        kind,
        location: Some(location.clone()),
        os_code,
        holders,
    }
}

#[cfg(windows)]
fn lock_holders(location: &Location) -> Vec<LockHolder> {
    os_path(location).map_or_else(|_| Vec::new(), |path| win_lock::holders(&path))
}

#[cfg(not(windows))]
fn lock_holders(_location: &Location) -> Vec<LockHolder> {
    Vec::new()
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
