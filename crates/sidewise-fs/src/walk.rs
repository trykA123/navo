//! Plain `std::fs` directory walk for the compare stack: one folder's entries, or one path's entry.
//!
//! Hosts without a provider (Sidewise's `StdSide`) list local folders through this module.
//! Entries follow `LocalFs` conventions: only files carry a size, and links are not followed.

use std::ffi::OsString;
use std::fs::{self, DirEntry, FileType, Metadata};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use navo_core::EntryKind;

#[cfg(test)]
mod tests;

/// One directory entry as the compare walk sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    pub name: OsString,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub link_to_folder: bool,
}

/// Lists `dir` with `std::fs::read_dir` + `symlink_metadata`, in OS order; checks `cancel` per entry.
///
/// Fails with the open or read error of `dir` (never an empty list for an unreadable folder),
/// and with `ErrorKind::Interrupted` once `cancel` is set. An entry deleted mid-walk is skipped.
pub fn list_dir(dir: &Path, cancel: &AtomicBool) -> io::Result<Vec<WalkEntry>> {
    check(cancel)?;
    let mut entries = Vec::new();
    for child in fs::read_dir(dir)? {
        check(cancel)?;
        if let Some(entry) = child_entry(&child?)? {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// One path's entry without following a final link; `link_to_folder` follows it once.
///
/// The name is the last path component, or the whole path for a root. Fails with the
/// `symlink_metadata` error, for example `NotFound`.
pub fn stat(path: &Path) -> io::Result<WalkEntry> {
    let metadata = fs::symlink_metadata(path)?;
    let name = path.file_name().unwrap_or(path.as_os_str()).to_owned();
    Ok(with_metadata(name, &metadata, path))
}

/// Returns `Interrupted` when the caller asked to stop.
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "walk cancelled"));
    }
    Ok(())
}

/// One listed child; `None` when it vanished between `read_dir` and `symlink_metadata`.
///
/// Other metadata errors keep the entry with its `read_dir` type and no size or time, as `LocalFs` does.
fn child_entry(child: &DirEntry) -> io::Result<Option<WalkEntry>> {
    let path = child.path();
    match fs::symlink_metadata(&path) {
        Ok(metadata) => Ok(Some(with_metadata(child.file_name(), &metadata, &path))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => match child.file_type() {
            Ok(file_type) => Ok(Some(bare(child.file_name(), file_type, &path))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        },
    }
}

/// Builds an entry from unfollowed metadata of `path`.
fn with_metadata(name: OsString, metadata: &Metadata, path: &Path) -> WalkEntry {
    let mut entry = bare(name, metadata.file_type(), path);
    if entry.kind == EntryKind::File {
        entry.size = metadata.len();
    }
    entry.modified_ms = modified_ms(metadata);
    entry
}

/// Builds an entry from its type alone: size 0, no time; a link is followed once for its target kind.
fn bare(name: OsString, file_type: FileType, path: &Path) -> WalkEntry {
    let kind = kind_of(file_type);
    WalkEntry {
        name,
        kind,
        size: 0,
        modified_ms: None,
        link_to_folder: kind == EntryKind::Link && fs::metadata(path).is_ok_and(|m| m.is_dir()),
    }
}

/// Maps an unfollowed file type to the compare kind; anything not a link or folder is a file.
fn kind_of(file_type: FileType) -> EntryKind {
    if file_type.is_symlink() {
        EntryKind::Link
    } else if file_type.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::File
    }
}

/// Modification time in milliseconds since the Unix epoch (negative before it); `None` if unknown.
fn modified_ms(metadata: &Metadata) -> Option<i64> {
    match metadata.modified().ok()?.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis())
            .ok()
            .map(i64::wrapping_neg),
    }
}
