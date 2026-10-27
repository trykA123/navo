use std::path::PathBuf;

use rusty_core::retention::RetainedDeletePlan;
use rusty_core::{FsError, FsErrorKind};
use rusty_fs::{JobLog, JobOutcome, Progress, StepOutcome};
use rusty_retention::{CaptureRequest, RetentionRun, Vault};
use rusty_ui::browser::LoggedOperation;

use super::{JournalLog, identity, lock};

mod recovery;
pub(super) use recovery::recover;

#[derive(Clone)]
pub(super) struct RetainedJob {
    pub data: PathBuf,
    pub plan: RetainedDeletePlan,
}

pub(super) fn execute(log: &mut JournalLog, mut operation: LoggedOperation<'_>) -> JobOutcome {
    let Some(job) = log.retained.clone() else {
        return failed(FsError::new(FsErrorKind::Unknown));
    };
    let outcome = capture(log, &job, &mut operation);
    if outcome != JobOutcome::Done {
        let restored = recovery::runtime_rollback(log, &job.data, operation.ops);
        log.put_back_failed = !restored;
    }
    outcome
}

fn capture(
    log: &mut JournalLog,
    job: &RetainedJob,
    operation: &mut LoggedOperation<'_>,
) -> JobOutcome {
    if job.plan.plan() != operation.plan {
        return failed(FsError::new(FsErrorKind::Unknown));
    }
    let result = capture_all(log, job, operation).and_then(|()| mark_steps(log, operation));
    match result {
        Ok(()) => JobOutcome::Done,
        Err(_) if operation.cancel.is_cancelled() => JobOutcome::Cancelled,
        Err(error) => {
            tracing::warn!(job = log.job.0, %error, "retained delete stopped; attempting rollback");
            failed(FsError::new(FsErrorKind::Unknown))
        }
    }
}

fn capture_all(
    log: &JournalLog,
    job: &RetainedJob,
    operation: &mut LoggedOperation<'_>,
) -> Result<(), String> {
    let (_, stored) = lock(&log.journal)
        .read_job(log.job)
        .map_err(|error| error.to_string())?;
    let (db, _) = crate::crash::open_db(&rusty_store::StoreDirs::journal_in(&job.data))
        .map_err(|error| error.to_string())?;
    let mut vault = Vault::new(db, &job.data, operation.ops).map_err(|error| error.to_string())?;
    let mut progress = initial_progress(operation.plan);
    for (seq, root) in job.plan.root_steps() {
        let identity = operation
            .ops
            .file_identity(&root)
            .map_err(|error| format!("{error:?}"))?
            .ok_or("the delete provider has no stable identity")?;
        identity::record(&log.journal, log.job, (seq, identity))
            .map_err(|error| format!("{error:?}"))?;
        let mut report = |_: rusty_retention::RetentionProgress| {
            progress.current = Some(root.clone());
            progress.paused = operation.cancel.is_paused();
            (operation.progress)(&progress);
            operation.cancel.wait_while_paused();
        };
        let entry = vault
            .capture(
                CaptureRequest {
                    source: &root,
                    policy: job.plan.policy(),
                    created_at: stored.created_at,
                    job: Some(log.job),
                },
                &mut RetentionRun::new(operation.cancel, &mut report),
            )
            .map_err(|error| error.to_string())?;
        if entry.record.nodes[0].identity != identity {
            return Err("the delete source changed after its journal intent".to_owned());
        }
    }
    Ok(())
}

fn initial_progress(plan: &rusty_core::OperationPlan) -> Progress {
    Progress::start(plan)
}

fn mark_steps(log: &mut JournalLog, operation: &mut LoggedOperation<'_>) -> Result<(), String> {
    for (seq, _) in (0_u32..).zip(&operation.plan.steps) {
        if operation.cancel.is_cancelled() {
            return Err("delete cancelled".to_owned());
        }
        log.step(seq, StepOutcome::Done)
            .map_err(|error| format!("{error:?}"))?;
    }
    (operation.progress)(&Progress::finished(operation.plan));
    if operation.cancel.is_cancelled() {
        return Err("delete cancelled".to_owned());
    }
    Ok(())
}

fn failed(error: FsError) -> JobOutcome {
    JobOutcome::Failed { seq: 0, error }
}

#[cfg(test)]
mod tests;
