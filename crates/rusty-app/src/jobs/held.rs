use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::{FileStamp, HeldFile, ProviderOps, release_held};
use rusty_store::{JobId, JournalError};
use serde::{Deserialize, Serialize};

use super::{SharedJournal, lock, now_ms};

#[derive(Serialize, Deserialize)]
struct HeldRecord {
    original: Location,
    held: Location,
    placed_size: u64,
    placed_modified_ms: Option<i64>,
}

pub(super) fn record(journal: &SharedJournal, job: JobId, held: &HeldFile) -> Result<(), FsError> {
    let record = HeldRecord {
        original: held.original.clone(),
        held: held.held.clone(),
        placed_size: held.placed.size,
        placed_modified_ms: held.placed.modified_ms,
    };
    let bytes = serde_json::to_vec(&record).map_err(|_| FsError::new(FsErrorKind::Unknown))?;
    lock(journal)
        .record_held(job, &bytes, now_ms())
        .map_err(|_| FsError::new(FsErrorKind::Unknown))
}

pub(super) fn held_of(
    journal: &SharedJournal,
    job: JobId,
) -> Result<Option<Vec<HeldFile>>, JournalError> {
    let rows = lock(journal).held(job)?;
    Ok(rows
        .iter()
        .map(|row| {
            let record = serde_json::from_slice::<HeldRecord>(row).ok()?;
            Some(HeldFile {
                original: record.original,
                held: record.held,
                placed: FileStamp {
                    size: record.placed_size,
                    modified_ms: record.placed_modified_ms,
                },
            })
        })
        .collect())
}

pub(super) fn release(
    journal: &SharedJournal,
    job: JobId,
    held: &[HeldFile],
    ops: &dyn ProviderOps,
) -> Result<(), JournalError> {
    if held.is_empty() {
        return Ok(());
    }
    if let Err(error) = release_held(held, ops) {
        tracing::warn!(job = job.0, error = ?error.kind, "held files not released");
        return Ok(());
    }
    lock(journal).forget_held(job).map(|_| ())
}

pub(super) fn release_finished(
    journal: &SharedJournal,
    ops: &dyn ProviderOps,
) -> Result<(), JournalError> {
    let jobs = lock(journal).finished_holding()?;
    for job in jobs {
        if let Some(held) = held_of(journal, job)? {
            release(journal, job, &held, ops)?;
        }
    }
    Ok(())
}
