use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::{FileStamp, HeldFile, ProviderOps, release_held};
use rusty_store::{JobId, JournalError};
use serde::{Deserialize, Serialize};

use super::{SharedJournal, lock, now_ms};

#[derive(Serialize, Deserialize)]
struct HeldRecord {
    original: Location,
    held: Location,
    #[serde(default)]
    placed_size: Option<u64>,
    #[serde(default)]
    placed_modified_ms: Option<i64>,
}

pub(super) struct Held {
    pub files: Vec<HeldFile>,
    pub unreadable: usize,
}

pub(super) fn record(journal: &SharedJournal, job: JobId, held: &HeldFile) -> Result<(), FsError> {
    let record = HeldRecord {
        original: held.original.clone(),
        held: held.held.clone(),
        placed_size: held.placed.map(|stamp| stamp.size),
        placed_modified_ms: held.placed.and_then(|stamp| stamp.modified_ms),
    };
    let bytes = serde_json::to_vec(&record).map_err(|_| FsError::new(FsErrorKind::Unknown))?;
    lock(journal)
        .record_held(job, &bytes, now_ms())
        .map_err(|_| FsError::new(FsErrorKind::Unknown))
}

pub(super) fn held_of(journal: &SharedJournal, job: JobId) -> Result<Held, JournalError> {
    let rows = lock(journal).held(job)?;
    let mut held = Held {
        files: Vec::new(),
        unreadable: 0,
    };
    for row in &rows {
        match serde_json::from_slice::<HeldRecord>(row) {
            Ok(record) => held.files.push(HeldFile {
                original: record.original,
                held: record.held,
                placed: record.placed_size.map(|size| FileStamp {
                    size,
                    modified_ms: record.placed_modified_ms,
                }),
            }),
            Err(error) => {
                held.unreadable += 1;
                tracing::warn!(job = job.0, %error, "unreadable held-file record kept");
            }
        }
    }
    Ok(held)
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
        let held = held_of(journal, job)?;
        if held.unreadable == 0 {
            release(journal, job, &held.files, ops)?;
        }
    }
    Ok(())
}
