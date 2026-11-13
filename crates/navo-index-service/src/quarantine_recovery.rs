//! Selects one validated recovery generation; T8 will connect this dormant API.
#![allow(dead_code, reason = "T8 activates recovery source selection")]
use std::{fs::File, io::Read, path::Path};

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
