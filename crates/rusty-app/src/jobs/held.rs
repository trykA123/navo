use rusty_core::retention::{OverwriteProof, ProvedOverwrite};
use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::{FileStamp, HeldFile, PlacedFile, ProviderOps, release_held};
use rusty_store::{JobId, JobState, JournalError};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    placed_hash: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    restoring: bool,
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
    write_record(journal, job, (held, false))
}

#[cfg(test)]
pub(super) fn record_restoring(
    journal: &SharedJournal,
    job: JobId,
    held: &HeldFile,
) -> Result<(), FsError> {
    write_record(journal, job, (held, true))
}

fn write_record(
    journal: &SharedJournal,
    job: JobId,
    (held, restoring): (&HeldFile, bool),
) -> Result<(), FsError> {
    let record = HeldRecord {
        original: held.original.clone(),
        held: held.held.clone(),
        placed_size: held.placed.map(|stamp| stamp.size),
        placed_modified_ms: held.placed.and_then(|stamp| stamp.modified_ms),
        volume: held.volume,
        proof: None,
        placed_hash: held.placed_hash,
        restoring,
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
                    placed_hash: record.placed_hash,
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
    has_key(journal, job, "proof")
}

pub(super) fn is_restoring(journal: &SharedJournal, job: JobId) -> Result<bool, JournalError> {
    has_key(journal, job, "restoring")
}

fn has_key(journal: &SharedJournal, job: JobId, key: &str) -> Result<bool, JournalError> {
    Ok(lock(journal).held(job)?.iter().any(|bytes| {
        serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .is_some_and(|value| value.get(key).is_some())
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
        release_job(journal, job, ops)?;
    }
    Ok(())
}

pub(super) fn release_job(
    journal: &SharedJournal,
    job: JobId,
    ops: &dyn ProviderOps,
) -> Result<(), JournalError> {
    let held = held_of(journal, job)?;
    let (state, stored) = lock(journal).read_job(job)?;
    if matches!(state, JobState::Running | JobState::RestorePending)
        || has_proof(journal, job)?
        || rusty_core::retention::is_retained_write(&stored.kind)
    {
        return Ok(());
    }
    if held.unreadable == 0 {
        if state == JobState::Undone && !discard_restoring(journal, job, ops)? {
            return Ok(());
        }
        release(journal, job, &held.files, ops)?;
    }
    Ok(())
}

fn discard_restoring(
    journal: &SharedJournal,
    job: JobId,
    ops: &dyn ProviderOps,
) -> Result<bool, JournalError> {
    let rows = lock(journal).held(job)?;
    for row in rows {
        let Ok(record) = serde_json::from_slice::<HeldRecord>(&row) else {
            continue;
        };
        if !record.restoring {
            continue;
        }
        if let Err(error) = ops.remove_file(&record.held)
            && error.kind != FsErrorKind::NotFound
        {
            tracing::warn!(job = job.0, error = ?error.kind, "discarded version not released");
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn hash_placed(
    journal: &SharedJournal,
    job: JobId,
    ops: &dyn ProviderOps,
) -> Result<(), String> {
    let rows = lock(journal).held(job).map_err(|error| error.to_string())?;
    for row in rows {
        let Row::Held(mut record) =
            serde_json::from_slice::<Row>(&row).map_err(|error| error.to_string())?
        else {
            continue;
        };
        let hash = ops
            .content_hash(&record.original, &rusty_fs::CancelToken::new())
            .map_err(|error| error.message())?;
        record.placed_hash = Some(hash);
        let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
        lock(journal)
            .database()
            .connection()
            .execute(
                "UPDATE undo SET inverse = ?3 WHERE job_id = ?1 AND inverse = ?2",
                (job.0, row.as_slice(), bytes.as_slice()),
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn settle_restore(journal: &SharedJournal, job: JobId) -> Result<(), JournalError> {
    let journal = lock(journal);
    let transaction = journal.database().connection().unchecked_transaction()?;
    transaction.execute("UPDATE job SET state = 'done' WHERE id = ?1", [job.0])?;
    for row in journal.held(job)? {
        if serde_json::from_slice::<HeldRecord>(&row).is_ok_and(|record| record.restoring) {
            transaction.execute(
                "DELETE FROM undo WHERE job_id = ?1 AND inverse = ?2",
                (job.0, row.as_slice()),
            )?;
        }
    }
    transaction.commit()?;
    Ok(())
}
