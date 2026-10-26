use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use super::{QuarantineReport, RecoveryError, io_error};

mod receipt;
mod record;

pub use receipt::acknowledge_quarantine;

use record::Pending;

/// Call before opening profile databases, while holding sole ownership of the profile.
pub fn quarantine_database(path: &Path) -> Result<Option<QuarantineReport>, RecoveryError> {
    let _lock = claim(path)?;
    if let Some(pending) = record::read(&suffixed(path, ".quarantine-receipt"))? {
        let report = receipt::report(path, pending)?;
        receipt::clear_pending(path, pending)?;
        return Ok(Some(report));
    }
    let record = suffixed(path, ".quarantine-pending");
    let pending = match record::read(&record)? {
        Some(pending) => pending,
        None => {
            let Some(pending) = prepare(path)? else {
                return Ok(None);
            };
            record::write(&record, pending)?;
            pending
        }
    };
    let report = move_files(path, pending)?;
    record::write(&suffixed(path, ".quarantine-receipt"), pending)?;
    fs::remove_file(&record)
        .map_err(|error| io_error("removing recovery record", &record, error))?;
    Ok(Some(report))
}

pub(super) fn claim(database: &Path) -> Result<File, RecoveryError> {
    let path = suffixed(database, ".quarantine-lock");
    regular(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| io_error("opening recovery lock", &path, error))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(RecoveryError::Busy(database.to_owned())),
        Err(TryLockError::Error(error)) => Err(io_error("locking recovery", &path, error)),
    }
}

fn prepare(path: &Path) -> Result<Option<Pending>, RecoveryError> {
    let database = regular(path)?;
    let wal = regular(&suffixed(path, "-wal"))?;
    let shm = regular(&suffixed(path, "-shm"))?;
    if !database && !wal && !shm {
        return Ok(None);
    }
    Ok(Some(Pending {
        slot: free_slot(path)?,
        database,
    }))
}

fn free_slot(path: &Path) -> Result<u64, RecoveryError> {
    for slot in 1..=u64::MAX {
        let target = suffixed(path, &format!(".corrupt-{slot}"));
        if !exists(&target)?
            && !exists(&suffixed(&target, "-wal"))?
            && !exists(&suffixed(&target, "-shm"))?
        {
            return Ok(slot);
        }
    }
    Err(io_error(
        "reserving quarantine slot",
        path,
        io::ErrorKind::AlreadyExists.into(),
    ))
}

fn move_files(path: &Path, pending: Pending) -> Result<QuarantineReport, RecoveryError> {
    let target = suffixed(path, &format!(".corrupt-{}", pending.slot));
    let database = if pending.database {
        move_file(path, &target, true)?
    } else {
        if exists(path)? || exists(&target)? {
            return Err(RecoveryError::InvalidRecord(suffixed(
                path,
                ".quarantine-pending",
            )));
        }
        None
    };
    let wal = move_file(&suffixed(path, "-wal"), &suffixed(&target, "-wal"), false)?;
    let shm = move_file(&suffixed(path, "-shm"), &suffixed(&target, "-shm"), false)?;
    Ok(QuarantineReport {
        slot: pending.slot,
        database,
        wal,
        shm,
    })
}

fn move_file(
    original: &Path,
    target: &Path,
    required: bool,
) -> Result<Option<PathBuf>, RecoveryError> {
    match (regular(original)?, regular(target)?) {
        (true, true) => {
            let identity = rusty_fs::native_identity(original)
                .map_err(|error| io_error("identifying preserved source", original, error))?;
            let preserved = rusty_fs::native_identity(target)
                .map_err(|error| io_error("identifying quarantine file", target, error))?;
            if identity.is_none() || identity != preserved {
                return Err(RecoveryError::AmbiguousMove {
                    original: original.to_owned(),
                    target: target.to_owned(),
                });
            }
            fs::remove_file(original)
                .map_err(|error| io_error("removing preserved source", original, error))?;
            Ok(Some(target.to_owned()))
        }
        (false, false) if required => Err(io_error(
            "finding quarantined database",
            target,
            io::ErrorKind::NotFound.into(),
        )),
        (false, false) => Ok(None),
        (false, true) => Ok(Some(target.to_owned())),
        (true, false) => {
            if rusty_fs::native_rename_atomic(original, target).is_err() {
                fs::hard_link(original, target)
                    .map_err(|error| io_error("preserving quarantine file", target, error))?;
                fs::remove_file(original)
                    .map_err(|error| io_error("removing preserved source", original, error))?;
            }
            Ok(Some(target.to_owned()))
        }
    }
}

pub(super) fn regular(path: &Path) -> Result<bool, RecoveryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(RecoveryError::NotFile(path.to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("inspecting recovery file", path, error)),
    }
}

fn exists(path: &Path) -> Result<bool, RecoveryError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("checking quarantine slot", path, error)),
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

#[cfg(test)]
mod tests;
