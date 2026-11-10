use std::sync::Arc;

use navo_core::OperationPlan;
use navo_store::{JobId, JobState};
use navo_ui::browser::OpLog;

use super::{JournalLog, SharedJournal, held, lock, profile};

#[cfg(test)]
pub fn undo_log(journal: &SharedJournal, job: JobId) -> Result<Box<dyn OpLog>, String> {
    Ok(Box::new(create_undo(journal, job)?))
}

pub fn profile_undo(
    journal: &SharedJournal,
    job: JobId,
    dirs: &navo_store::StoreDirs,
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

impl JournalLog {
    /// Restores every old file of this Save job, newest written step first.
    #[expect(
        clippy::too_many_arguments,
        reason = "the hook passes the plan, its done steps and the operation context"
    )]
    pub(super) fn restore_saved(
        &mut self,
        plan: &OperationPlan,
        done: &[u32],
        ops: &dyn navo_fs::ProviderOps,
        cancel: &navo_fs::CancelToken,
    ) -> Result<navo_fs::SaveUndo, navo_core::FsError> {
        let rows = held::held_of(&self.journal, self.job)
            .map_err(|_| navo_core::FsError::new(navo_core::FsErrorKind::Unknown))?;
        if rows.unreadable > 0 {
            return Err(navo_core::FsError::new(
                navo_core::FsErrorKind::NotSupportedYet,
            ));
        }
        let journal = Arc::clone(&self.journal);
        let job = self.job;
        let mut outcome = navo_fs::StepOutcome::Done;
        let mut restored = 0_u32;
        let mut missing = 0_u32;
        let mut ctx = navo_fs::JobContext {
            ops,
            log: self,
            cancel,
            progress: &mut |_| {},
        };
        for (index, step) in plan.steps.iter().enumerate().rev() {
            let seq = u32::try_from(index).unwrap_or(u32::MAX);
            if !done.contains(&seq) {
                continue;
            }
            let navo_core::Step::WriteFile { target, .. } = step else {
                continue;
            };
            let Some(file) = rows.files.iter().find(|file| file.original == *target) else {
                missing += 1;
                continue;
            };
            let step_outcome = navo_fs::undo_write(file, &mut ctx)?;
            restored += 1;
            lock(&journal)
                .mark_undone(job, seq)
                .map_err(|_| navo_core::FsError::new(navo_core::FsErrorKind::Unknown))?;
            if step_outcome != navo_fs::StepOutcome::Done {
                outcome = step_outcome;
            }
        }
        if restored == 0 && missing == 0 {
            return Err(navo_core::FsError::new(navo_core::FsErrorKind::NotFound));
        }
        Ok(navo_fs::SaveUndo { outcome, missing })
    }
}
