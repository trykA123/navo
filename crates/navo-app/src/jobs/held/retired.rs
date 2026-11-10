//! Forgets the undo rows of a retained Save once its vault entry no longer holds the old file.
//! Failed saves keep their rows: stranded-save recovery still reads them.

use std::path::Path;

use navo_core::Location;
use navo_core::retention::{RetainedState, SAVE_RETAINED_KIND};
use navo_store::{JobId, JobState, RetentionCatalog, StoreDirs};

use super::{FORGET_ROW, HeldRecord, SharedJournal, lock};

/// Forgets the retired rows of every finished retained Save in the profile at `data`.
/// Returns the number of rows removed; fails when the journal or the catalog cannot be read.
fn forget_retired_saves(journal: &SharedJournal, data: &Path) -> Result<usize, String> {
    let jobs = lock(journal)
        .finished_holding()
        .map_err(|error| error.to_string())?;
    let catalog = open_catalog(data)?;
    let mut removed = 0;
    for job in jobs {
        removed += forget_in(journal, &catalog, job)?;
    }
    Ok(removed)
}

/// After a maintenance pass: forgets the rows of the saves it trimmed, and logs the result.
/// Opens the profile journal when the caller has none; never fails the pass.
pub(crate) fn after_trim(journal: Option<&SharedJournal>, data: &Path) {
    let opened;
    let journal = match journal {
        Some(journal) => journal,
        None => match crate::jobs::open(&StoreDirs::journal_in(data)) {
            Ok(own) => {
                opened = own;
                &opened
            }
            Err(error) => {
                tracing::warn!(%error, "undo rows of trimmed saves kept");
                return;
            }
        },
    };
    match forget_retired_saves(journal, data) {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, "undo rows of trimmed saves forgotten"),
        Err(error) => tracing::warn!(%error, "undo rows of trimmed saves kept"),
    }
}

/// Forgets the retired rows of retained Save `job`; returns how many were removed.
pub(crate) fn forget_retired(
    journal: &SharedJournal,
    data: &Path,
    job: JobId,
) -> Result<usize, String> {
    forget_in(journal, &open_catalog(data)?, job)
}

/// Forgets every held row of `job` that points at `held`; returns how many were removed.
pub(crate) fn forget_held_at(
    journal: &SharedJournal,
    job: JobId,
    held: &Location,
) -> Result<usize, String> {
    let rows: Vec<Vec<u8>> = lock(journal)
        .held(job)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|row| record_of(row).is_some_and(|record| record.held.same_place(held)))
        .collect();
    delete_rows(journal, job, &rows)
}

/// Opens the retention catalog of the profile at `data`.
fn open_catalog(data: &Path) -> Result<RetentionCatalog, String> {
    let (db, _) =
        crate::crash::open_db(&StoreDirs::journal_in(data)).map_err(|error| error.to_string())?;
    RetentionCatalog::new(db).map_err(|error| error.to_string())
}

/// Removes the rows of one finished retained Save whose vault entry is restored, purged or pruned.
fn forget_in(
    journal: &SharedJournal,
    catalog: &RetentionCatalog,
    job: JobId,
) -> Result<usize, String> {
    let (state, stored) = lock(journal)
        .read_job(job)
        .map_err(|error| error.to_string())?;
    if stored.kind != SAVE_RETAINED_KIND
        || matches!(
            state,
            JobState::Running | JobState::RestorePending | JobState::Failed
        )
    {
        return Ok(0);
    }
    let entries = payloads(catalog, job)?;
    let rows: Vec<Vec<u8>> = lock(journal)
        .held(job)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|row| record_of(row).is_some_and(|record| retired(&record, &entries)))
        .collect();
    delete_rows(journal, job, &rows)
}

/// `true` when the row's vault file is gone: its entry is restored or purged, or a proof row
/// has no entry left because the catalog pruned it. A `.current` row has no proof and stays.
fn retired(record: &HeldRecord, entries: &[(Location, bool)]) -> bool {
    match entries.iter().find(|(at, _)| at.same_place(&record.held)) {
        Some((_, live)) => !live,
        None => record.proof.is_some(),
    }
}

/// The vault file of each entry `job` owns, and whether that entry still holds it.
fn payloads(catalog: &RetentionCatalog, job: JobId) -> Result<Vec<(Location, bool)>, String> {
    let mut entries = Vec::new();
    for id in catalog
        .ids_for_job(job)
        .map_err(|error| error.to_string())?
    {
        let entry = catalog.get(id).map_err(|error| error.to_string())?;
        let live = !matches!(entry.state, RetainedState::Restored | RetainedState::Purged);
        let held = entry.record.held().map_err(|error| error.to_string())?;
        entries.push((held, live));
    }
    Ok(entries)
}

/// The held-file record in one undo row, or `None` for a placed row or an unreadable one.
fn record_of(row: &[u8]) -> Option<HeldRecord> {
    serde_json::from_slice::<HeldRecord>(row).ok()
}

/// Deletes `rows` of `job` in one transaction; returns how many were removed.
fn delete_rows(journal: &SharedJournal, job: JobId, rows: &[Vec<u8>]) -> Result<usize, String> {
    if rows.is_empty() {
        return Ok(0);
    }
    let journal = lock(journal);
    let transaction = journal
        .database()
        .connection()
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let mut removed = 0;
    for row in rows {
        removed += transaction
            .execute(FORGET_ROW, (job.0, row.as_slice()))
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(removed)
}
