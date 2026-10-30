use std::collections::BTreeMap;
use std::path::Path;

use rusty_fs::ProviderOps;
use rusty_store::{JobId, JobState, JournalError, UnfinishedJob};

use super::{SharedJournal, StillPending, held, lock, pending};

mod legacy;
mod moves;
use legacy::undo;

pub struct RecoveryReport {
    pub unfinished: u32,
    pub restored: Vec<JobId>,
    pub pending: Vec<StillPending>,
}

pub fn recover_profile(
    journal: &SharedJournal,
    ops: &dyn ProviderOps,
    scope: (&Path, &[JobId]),
    experimental: bool,
) -> Result<RecoveryReport, JournalError> {
    recover_in(journal, ops, (scope.0, scope.1, experimental))
}

fn recover_in(
    journal: &SharedJournal,
    ops: &dyn ProviderOps,
    (data, expected, experimental): (&Path, &[JobId], bool),
) -> Result<RecoveryReport, JournalError> {
    let unfinished = lock(journal).unfinished()?;
    let mut outcomes = BTreeMap::new();
    for &id in expected {
        let (state, job) = lock(journal).read_job(id)?;
        outcomes.insert(id, (job, state));
    }
    for job in unfinished.iter().rev() {
        recover_one(journal, (ops, data, experimental), (job, &mut outcomes))?;
    }
    let pending = lock(journal).restore_pending()?;
    for job in pending.iter().rev() {
        recover_one(journal, (ops, data, experimental), (job, &mut outcomes))?;
    }
    held::release_finished(journal, ops)?;
    let mut report = RecoveryReport {
        unfinished: u32::try_from(unfinished.len()).unwrap_or(u32::MAX),
        restored: Vec::new(),
        pending: Vec::new(),
    };
    for (id, (job, state)) in outcomes {
        match state {
            JobState::RolledBack | JobState::Undone => report.restored.push(id),
            JobState::Failed if rusty_core::retention::is_retained_write(&job.kind) => (),
            _ => report.pending.push(pending::details(journal, &job)?),
        }
    }
    Ok(report)
}

fn recover_one(
    journal: &SharedJournal,
    (ops, data, experimental): (&dyn ProviderOps, &Path, bool),
    (job, outcomes): (
        &UnfinishedJob,
        &mut BTreeMap<JobId, (UnfinishedJob, JobState)>,
    ),
) -> Result<(), JournalError> {
    let retained = job.kind == rusty_core::retention::RETAINED_DELETE_KIND
        || rusty_core::retention::is_retained_write(&job.kind);
    let state = if retained && !experimental {
        JobState::RestorePending
    } else if job.kind == rusty_core::retention::RETAINED_DELETE_KIND {
        super::deletion::recover(journal, job, (data, ops))
    } else if rusty_core::retention::is_retained_write(&job.kind) {
        super::overwrite::recover(journal, job, (data, ops))
    } else if has_retention(&job.plan)
        || has_unrecognized_detail(job)
        || super::held::has_proof(journal, job.id)?
        || !lock(journal).retained_paths(job.id)?.is_empty()
    {
        JobState::RestorePending
    } else {
        undo(journal, job, ops, experimental)?
    };
    lock(journal).finish(job.id, state)?;
    outcomes.insert(job.id, (job.clone(), state));
    Ok(())
}

fn has_retention(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .is_some_and(|value| {
            value.get("retention_version").is_some()
                || value.get("retention").is_some()
                || value.get("overwrite_retention_version").is_some()
                || value.get("temporary_namespace").is_some()
        })
}

fn has_unrecognized_detail(job: &UnfinishedJob) -> bool {
    let renames = serde_json::from_slice::<rusty_core::OperationPlan>(&job.plan)
        .ok()
        .filter(|plan| {
            matches!(
                plan.kind,
                rusty_core::OpKind::Rename | rusty_core::OpKind::Move
            )
        });
    job.steps.iter().any(|step| {
        let Some(bytes) = step.detail.as_deref() else {
            return false;
        };
        if bytes == b"undone" {
            return false;
        }
        !renames.as_ref().is_some_and(|plan| {
            matches!(
                plan.steps.get(step.seq as usize),
                Some(rusty_core::Step::Rename { .. })
            ) && super::identity::decode(bytes).is_ok_and(|identity| identity.is_some())
        })
    })
}
