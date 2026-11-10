// In-place undo for retained Save jobs: an unchanged target gets its old bytes back.
// A changed target keeps the J3b beside rule, so this path only runs when the hash matches.

use navo_core::retention::{
    RestoreTarget, RetainedState as Stored, RetentionEntry, SAVE_RETAINED_KIND,
};
use navo_core::{FsError, Location};
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

/// Puts the old file back at its original path, keeping the newer bytes in the job hold.
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
    mark_restored(dirs, entry, ops)?;
    Ok(entry.record.source.clone())
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
