//! Selects recovery generations and prepares stopped-service rollback; dormant until T8.
#![allow(dead_code, reason = "T8 activates recovery source selection")]
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use navo_index::{
    file::{self, FileError},
    ntfs::{NtfsError, NtfsIndex},
};

use crate::{
    config::Drive,
    quarantine::{self, Identity, Marker},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Canonical,
    Backup,
}

#[derive(Debug)]
pub(crate) struct Recovery {
    pub source: Source,
    pub identity: Identity,
    /// Canonical full storage permits live journal validation, never unconditional replay.
    pub index: NtfsIndex,
}

/// Stopped-service snapshot paths and drive identity for an MQ2 rollback.
#[derive(Debug)]
pub(crate) struct Rollback<'a> {
    pub path: &'a Path,
    pub preserved: &'a Path,
    pub drive: Drive,
}

/// Preserves stopped-service files, then cold-starts MQ2; any preservation failure forbids startup.
#[cfg(any(windows, test))]
pub(crate) fn rollback(
    request: Rollback<'_>,
    enumerate: impl FnOnce(navo_index::pipe::BuildReason) -> Result<NtfsIndex, NtfsError>,
) -> Result<NtfsIndex, NtfsError> {
    preserve_rollback(request.path, request.preserved).map_err(FileError::from)?;
    crate::lifecycle::restore_or_enumerate(
        file::load_with_ids(request.path),
        request.drive,
        enumerate,
    )
}

/// Copies the stopped snapshot set into a fresh directory; errors retain every unpreserved source.
fn preserve_rollback(path: &Path, preserved: &Path) -> io::Result<()> {
    fs::create_dir(preserved)?;
    #[cfg(not(windows))]
    File::open(preserved.join(".."))?.sync_all()?;
    for suffix in [
        "",
        ".bak",
        ".rebuilding",
        ".rebuilding.pending",
        ".rebuilding.tmp",
    ] {
        let mut active = path.as_os_str().to_owned();
        active.push(suffix);
        let active = PathBuf::from(active);
        if !active.try_exists()? {
            continue;
        }
        let name = active.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        preserve_file(&active, &preserved.join(name))?;
    }
    for active in [path.to_path_buf(), navo_store::backup_path(path)] {
        if active.try_exists()? {
            return Err(io::Error::other("rollback snapshot remains active"));
        }
    }
    #[cfg(not(windows))]
    File::open(path.with_file_name("."))?.sync_all()?;
    Ok(())
}

/// Streams and syncs a preserved copy before unlinking its source; ambiguous errors keep both.
fn preserve_file(active: &Path, saved: &Path) -> io::Result<()> {
    let mut temporary = saved.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let mut source = File::open(active)?;
    let mut target = File::options()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    io::copy(&mut source, &mut target)?;
    target.sync_all()?;
    drop(target);
    drop(source);
    navo_fs::native_replace_durable(&temporary, saved)?;
    fs::remove_file(active)
}

/// Loads canonical or backup for `drive`; the caller serializes replacement through S13 repair.
/// Backup success acknowledges active quarantine; errors forbid repair, checkpoint and conversion.
pub(crate) fn load(path: &Path, drive: Drive) -> Result<Recovery, NtfsError> {
    let (source, identity, mut index) = match load_source(path, drive) {
        Ok((identity, index)) => (Source::Canonical, identity, index),
        Err(_) => {
            let (identity, index) = load_source(&navo_store::backup_path(path), drive)?;
            (Source::Backup, identity, index)
        }
    };
    let search_only = match source {
        Source::Backup => {
            quarantine::write_active(path, identity).map_err(FileError::from)?;
            true
        }
        Source::Canonical => {
            quarantine::read(path, identity).map_err(FileError::from)? == Marker::SearchOnly
        }
    };
    if search_only {
        index.make_search_only();
    }
    Ok(Recovery {
        source,
        identity,
        index,
    })
}

/// Reads the fixed v3 prefix without allocating snapshot storage; errors reject this source.
fn prefix(path: &Path) -> Result<[u8; 70], FileError> {
    let mut bytes = [0; 70];
    File::open(path)?.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Validates checksum, drive and restorable structure once; changed prefixes reject the source.
fn load_source(path: &Path, drive: Drive) -> Result<(Identity, NtfsIndex), NtfsError> {
    let before = prefix(path)?;
    let saved = file::load_with_ids(path)?;
    if before != prefix(path)? || saved.0.root != format!("{}:", drive.letter) {
        return Err(FileError::InvalidData("snapshot changed or has the wrong root").into());
    }
    let identity = Identity {
        serial: saved.0.volume_serial,
        journal: saved.0.journal_id,
        cursor: saved.0.next_usn,
        checksum: u32::from_le_bytes([before[66], before[67], before[68], before[69]]),
    };
    let index = NtfsIndex::restore(saved, drive.serial, identity.journal)?;
    Ok((identity, index))
}

#[cfg(test)]
#[path = "quarantine_recovery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "quarantine_rollback_tests.rs"]
mod rollback_tests;
