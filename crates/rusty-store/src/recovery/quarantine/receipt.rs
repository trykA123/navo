use std::fs;
use std::path::{Path, PathBuf};

use super::{Pending, QuarantineReport, RecoveryError, claim, io_error, record, regular, suffixed};

pub fn acknowledge_quarantine(path: &Path, slot: u64) -> Result<(), RecoveryError> {
    let _lock = claim(path)?;
    let receipt = suffixed(path, ".quarantine-receipt");
    let Some(pending) = record::read(&receipt)? else {
        return Ok(());
    };
    if pending.slot != slot {
        return Err(RecoveryError::InvalidRecord(receipt));
    }
    report(path, pending)?;
    clear_pending(path, pending)?;
    fs::remove_file(&receipt).map_err(|error| io_error("acknowledging quarantine", &receipt, error))
}

pub(super) fn clear_pending(path: &Path, receipt: Pending) -> Result<(), RecoveryError> {
    let path = suffixed(path, ".quarantine-pending");
    let Some(pending) = record::read(&path)? else {
        return Ok(());
    };
    if pending != receipt {
        return Err(RecoveryError::InvalidRecord(path));
    }
    fs::remove_file(&path).map_err(|error| io_error("completing quarantine record", &path, error))
}

pub(super) fn report(path: &Path, pending: Pending) -> Result<QuarantineReport, RecoveryError> {
    let target = suffixed(path, &format!(".corrupt-{}", pending.slot));
    let database = preserved(&target)?;
    let wal = preserved(&suffixed(&target, "-wal"))?;
    let shm = preserved(&suffixed(&target, "-shm"))?;
    if database.is_some() != pending.database
        || (database.is_none() && wal.is_none() && shm.is_none())
    {
        return Err(RecoveryError::InvalidRecord(suffixed(
            path,
            ".quarantine-receipt",
        )));
    }
    Ok(QuarantineReport {
        slot: pending.slot,
        database,
        wal,
        shm,
    })
}

fn preserved(path: &Path) -> Result<Option<PathBuf>, RecoveryError> {
    Ok(regular(path)?.then(|| path.to_owned()))
}
