use std::sync::Arc;

use rusty_core::OperationPlan;
use rusty_store::{JobId, JobState};
use rusty_ui::browser::OpLog;

use super::{JournalLog, SharedJournal, lock, profile};

#[cfg(test)]
pub fn undo_log(journal: &SharedJournal, job: JobId) -> Result<Box<dyn OpLog>, String> {
    Ok(Box::new(create_undo(journal, job)?))
}

pub fn profile_undo(
    journal: &SharedJournal,
    job: JobId,
    dirs: &rusty_store::StoreDirs,
    experimental: bool,
) -> Result<Box<dyn OpLog>, String> {
    let (_, stored) = lock(journal)
        .read_job(job)
        .map_err(|error| error.to_string())?;
    let plan =
        serde_json::from_slice::<OperationPlan>(&stored.plan).map_err(|error| error.to_string())?;
    profile::guard(&plan, dirs, experimental)?;
    let mut log = JournalLog::new(Arc::clone(journal), job, "rename undo".to_owned());
    log.undoing = true;
    log.renames = true;
    log.deferred_undo = true;
    log.profile = Some(dirs.clone());
    log.experimental_retention = experimental;
    Ok(Box::new(log))
}

#[cfg(test)]
fn create_undo(journal: &SharedJournal, job: JobId) -> Result<JournalLog, String> {
    lock(journal)
        .finish(job, JobState::RestorePending)
        .map_err(|error| error.to_string())?;
    Ok(undo_record(journal, job))
}

pub(crate) fn claim_undo_log(
    journal: &SharedJournal,
    job: JobId,
) -> Result<Box<dyn OpLog>, String> {
    crate::rename_hooks::claim_undo(journal, job, JobState::RestorePending)?;
    Ok(Box::new(undo_record(journal, job)))
}

fn undo_record(journal: &SharedJournal, job: JobId) -> JournalLog {
    let mut log = JournalLog::new(Arc::clone(journal), job, "rename undo".to_owned());
    log.undoing = true;
    log.renames = true;
    log
}
