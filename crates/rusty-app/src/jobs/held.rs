use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::{FileStamp, HeldFile, PlacedFile, ProviderOps, release_held};
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

#[derive(Serialize, Deserialize)]
struct PlacedRecord {
    placed_target: Location,
    size: u64,
    #[serde(default)]
    modified_ms: Option<i64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Row {
    Placed(PlacedRecord),
    Held(HeldRecord),
}

pub(super) struct Held {
    pub files: Vec<HeldFile>,
    pub placed: Vec<PlacedFile>,
    pub unreadable: usize,
}

pub(super) fn record_placed(
    journal: &SharedJournal,
    job: JobId,
    placed: &PlacedFile,
) -> Result<(), FsError> {
    let record = PlacedRecord {
        placed_target: placed.target.clone(),
        size: placed.stamp.size,
        modified_ms: placed.stamp.modified_ms,
    };
    let bytes = serde_json::to_vec(&record).map_err(|_| FsError::new(FsErrorKind::Unknown))?;
    lock(journal)
        .record_held(job, &bytes, now_ms())
        .map_err(|_| FsError::new(FsErrorKind::Unknown))
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
        placed: Vec::new(),
        unreadable: 0,
    };
    for row in &rows {
        match serde_json::from_slice::<Row>(row) {
            Ok(Row::Held(record)) => held.files.push(HeldFile {
                original: record.original,
                held: record.held,
                placed: record.placed_size.map(|size| FileStamp {
                    size,
                    modified_ms: record.placed_modified_ms,
                }),
            }),
            Ok(Row::Placed(record)) => held.placed.push(PlacedFile {
                target: record.placed_target,
                stamp: FileStamp {
                    size: record.size,
                    modified_ms: record.modified_ms,
                },
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
    if !held.is_empty()
        && let Err(error) = release_held(held, ops)
    {
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
