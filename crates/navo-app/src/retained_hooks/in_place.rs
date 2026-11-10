// In-place undo for retained Save jobs: an unchanged target gets its old bytes back.
// A changed target keeps the J3b beside rule, so this path only runs when the hash matches.

use navo_core::retention::{
    RestoreTarget, RetainedState as Stored, RetentionEntry, SAVE_RETAINED_KIND,
};
use navo_core::{FsError, FsErrorKind, Location, ProviderPath};
use navo_fs::{CancelToken, HeldFile, JobContext, JobLog, ProviderOps, StepOutcome};
use navo_store::{JobId, RetentionCatalog, StoreDirs};

/// The held old file of a retained Save job, when the journal recorded its placed hash.
pub(super) fn saved_held(
    dirs: &StoreDirs,
    entry: &RetentionEntry,
) -> Result<Option<HeldFile>, String> {
    let Some(job) = entry.job_id else {
        return Ok(None);
    };
    let journal = crate::jobs::open(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let (_, stored) = crate::jobs::lock(&journal)
        .read_job(JobId(job))
        .map_err(|error| error.to_string())?;
    if stored.kind != SAVE_RETAINED_KIND {
        return Ok(None);
    }
    crate::jobs::save_held(&journal, JobId(job), &entry.record.source)
        .map_err(|error| error.to_string())
}

/// Puts the old file back at its original path and drops the newer bytes (no redo).
/// Fails when undo refuses, or when the entry cannot be marked restored after one retry.
pub(super) fn restore(
    dirs: &StoreDirs,
    ops: &dyn ProviderOps,
    entry: &RetentionEntry,
    file: &HeldFile,
) -> Result<Location, String> {
    let cancel = CancelToken::new();
    let journal = crate::jobs::open(&dirs.journal_file()).map_err(|error| error.to_string())?;
    let Some(job) = entry.job_id else {
        return Err(navo_l10n::tr("retained-not-ready"));
    };
    let mut log = SaveUndoLog {
        journal,
        job: JobId(job),
    };
    let mut ctx = JobContext {
        ops,
        log: &mut log,
        cancel: &cancel,
        progress: &mut |_| {},
    };
    navo_fs::undo_write(file, &mut ctx).map_err(|error| error.message())?;
    mark_restored(dirs, entry, ops).or_else(|error| {
        tracing::warn!(id = entry.id.0, %error, "restored entry not marked; retrying once");
        mark_restored(dirs, entry, ops)
    })?;
    forget_newer(&log.journal, (dirs, log.job), file, ops);
    Ok(entry.record.source.clone())
}

/// Forgets the job rows of a restored entry, then removes the newer bytes kept as `.current`.
/// Rows go first, so a crash leaves an orphan file, never a row without its file.
fn forget_newer(
    journal: &crate::jobs::SharedJournal,
    (dirs, job): (&StoreDirs, JobId),
    file: &HeldFile,
    ops: &dyn ProviderOps,
) {
    let Some(current) = current_of(&file.held) else {
        return;
    };
    if let Err(error) = crate::jobs::forget_retired(journal, &dirs.data, job) {
        tracing::warn!(job = job.0, %error, "undo row of a restored save kept");
    }
    if let Err(error) = crate::jobs::forget_held_at(journal, job, &current) {
        tracing::warn!(job = job.0, %error, "newer bytes of a restored save kept");
        return;
    }
    if let Err(error) = ops.remove_file(&current)
        && error.kind != FsErrorKind::NotFound
    {
        tracing::warn!(job = job.0, error = ?error.kind, "newer bytes of a restored save kept");
    }
}

/// Where in-place undo parks the newer bytes: the held file's name plus `.current`.
fn current_of(held: &Location) -> Option<Location> {
    let name = held.name()?.to_text();
    held.parent()?
        .join(&ProviderPath::from_text(&format!("{name}.current")))
        .ok()
}

/// Records the file that an in-place undo moved out of the way.
struct SaveUndoLog {
    journal: crate::jobs::SharedJournal,
    job: JobId,
}

impl JobLog for SaveUndoLog {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, file: &HeldFile) -> Result<bool, FsError> {
        crate::jobs::hold_record(&self.journal, self.job, file)?;
        Ok(true)
    }
}

/// Marks a restored retained Save entry as restored in the catalog.
fn mark_restored(
    dirs: &StoreDirs,
    entry: &RetentionEntry,
    ops: &dyn ProviderOps,
) -> Result<(), String> {
    let parent = entry
        .record
        .source
        .parent()
        .ok_or_else(|| navo_l10n::tr("retained-not-ready"))?;
    let parent_identity = ops
        .file_identity(&parent)
        .map_err(|error| error.message())?
        .ok_or_else(|| navo_l10n::tr("retained-not-ready"))?;
    let (db, _) = crate::crash::open_db(&dirs.journal_file())
        .map_err(|error| navo_l10n::t!("retained-open-failed", error = error.to_string()))?;
    let mut catalog = RetentionCatalog::new(db).map_err(|error| error.to_string())?;
    let mut tx = catalog.transaction().map_err(|error| error.to_string())?;
    let mut restoring = tx.get(entry.id).map_err(|error| error.to_string())?;
    restoring.target = Some(RestoreTarget {
        location: entry.record.source.clone(),
        parent_identity,
    });
    restoring.state = Stored::Restoring;
    tx.save(&restoring, Stored::Ready)
        .map_err(|error| error.to_string())?;
    let mut restored = tx.get(entry.id).map_err(|error| error.to_string())?;
    restored.state = Stored::Restored;
    tx.save(&restored, Stored::Restoring)
        .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}
