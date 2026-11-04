use std::collections::BTreeMap;
use std::path::Path;

use navo_fs::ProviderOps;
use navo_store::{JobId, JobState, JournalError, UnfinishedJob};

use super::{SharedJournal, StillPending, held, lock, pending};

mod legacy;
mod moves;
pub(super) mod stranded;
use legacy::undo;

type Outcome = (UnfinishedJob, JobState, bool);

pub(super) const FAILED_SAVES_LISTED: u32 = 200;

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
    let _recovering = super::trim::recovering(journal);
    let unfinished = lock(journal).unfinished()?;
    let mut outcomes = BTreeMap::new();
    for &id in expected {
        let (state, job) = lock(journal).read_job(id)?;
        outcomes.insert(id, (job, state, false));
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
    for (id, (job, state, stranded)) in outcomes {
        let settled = state == JobState::Failed
            && navo_core::retention::is_retained_write(&job.kind)
            && !stranded;
        match state {
            JobState::RolledBack | JobState::Undone => report.restored.push(id),
            _ if settled => (),
            _ => {
                let mut left = pending::details(journal, &job)?;
                left.attention = state == JobState::Failed
                    && job.kind == navo_core::retention::RETAINED_DELETE_KIND;
                report.pending.push(left);
            }
        }
    }
    stranded::list(journal, ops, &mut report.pending)?;
    Ok(report)
}

fn recover_one(
    journal: &SharedJournal,
    (ops, data, experimental): (&dyn ProviderOps, &Path, bool),
    (job, outcomes): (&UnfinishedJob, &mut BTreeMap<JobId, Outcome>),
) -> Result<(), JournalError> {
    let retained = job.kind == navo_core::retention::RETAINED_DELETE_KIND
        || navo_core::retention::is_retained_write(&job.kind);
    let mut stranded = false;
    let state = if retained && !experimental {
        JobState::RestorePending
    } else if job.kind == navo_core::retention::RETAINED_DELETE_KIND {
        super::deletion::recover(journal, job, (data, ops))
    } else if navo_core::retention::is_retained_write(&job.kind) {
        let recovered = super::overwrite::recover(journal, job, (data, ops));
        stranded = recovered.1;
        recovered.0
    } else if has_retention(&job.plan)
        || has_unrecognized_detail(job)
        || super::held::has_proof(journal, job.id)?
        || super::held::is_restoring(journal, job.id)?
        || !lock(journal).retained_paths(job.id)?.is_empty()
    {
        JobState::RestorePending
    } else {
        undo(journal, job, ops, experimental)?
    };
    lock(journal).finish(job.id, state)?;
    outcomes.insert(job.id, (job.clone(), state, stranded));
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
    let renames = serde_json::from_slice::<navo_core::OperationPlan>(&job.plan)
        .ok()
        .filter(|plan| {
            matches!(
                plan.kind,
                navo_core::OpKind::Rename | navo_core::OpKind::Move
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
                Some(navo_core::Step::Rename { .. })
            ) && super::identity::decode(bytes).is_ok_and(|identity| identity.is_some())
        })
    })
}
