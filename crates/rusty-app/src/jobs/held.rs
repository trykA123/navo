use rusty_core::retention::{OverwriteProof, ProvedOverwrite};
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
    #[serde(default)]
    volume: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proof: Option<OverwriteProof>,
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
    Held(Box<HeldRecord>),
}

pub(super) struct Held {
    pub files: Vec<HeldFile>,
    pub placed: Vec<PlacedFile>,
    pub unreadable: usize,
    pub proved: Vec<(HeldFile, OverwriteProof)>,
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
        volume: held.volume,
        proof: None,
    };
    let bytes = serde_json::to_vec(&record).map_err(|_| FsError::new(FsErrorKind::Unknown))?;
    lock(journal)
        .record_held(job, &bytes, now_ms())
        .map_err(|_| FsError::new(FsErrorKind::Unknown))
}

pub(super) fn record_proved(
    journal: &SharedJournal,
    job: JobId,
    (held, proof): (&HeldFile, &OverwriteProof),
) -> Result<(), FsError> {
    let record = ProvedOverwrite {
        original: held.original.clone(),
        held: held.held.clone(),
        placed_size: held.placed.map(|stamp| stamp.size),
        placed_modified_ms: held.placed.and_then(|stamp| stamp.modified_ms),
        volume: held.volume,
        proof: proof.clone(),
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
        proved: Vec::new(),
    };
    for row in &rows {
        match serde_json::from_slice::<Row>(row) {
            Ok(Row::Held(record)) => {
                let file = HeldFile {
                    original: record.original,
                    held: record.held,
                    placed: record.placed_size.map(|size| FileStamp {
                        size,
                        modified_ms: record.placed_modified_ms,
                    }),
                    volume: record.volume,
                };
                if let Some(proof) = record.proof {
                    held.proved.push((file.clone(), proof));
                }
                held.files.push(file);
            }
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

pub(super) fn has_proof(journal: &SharedJournal, job: JobId) -> Result<bool, JournalError> {
    Ok(lock(journal).held(job)?.iter().any(|bytes| {
        serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .is_some_and(|value| value.get("proof").is_some())
    }))
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
        let (_, stored) = lock(journal).read_job(job)?;
        if has_proof(journal, job)? || rusty_core::retention::is_retained_write(&stored.kind) {
            continue;
        }
        if held.unreadable == 0 {
            release(journal, job, &held.files, ops)?;
        }
    }
    Ok(())
}
