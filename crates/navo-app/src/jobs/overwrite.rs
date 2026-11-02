use std::path::{Path, PathBuf};
use std::sync::Arc;

use navo_core::retention::{OverwriteProof, RetainedWritePlan};
use navo_core::{Location, OpKind, OperationPlan, Step};
use navo_fs::{CancelToken, ProviderOps};
use navo_retention::{AdoptJob, RetentionRun, Vault};
use navo_store::{Db, JobId, JobState, UnfinishedJob};

use super::{JournalLog, SharedJournal, held, lock, now_ms};

pub(super) struct WriteJob {
    pub data: PathBuf,
    pub plan: RetainedWritePlan,
}

pub(super) fn create(journal: &SharedJournal, mut request: WriteJob) -> Result<JournalLog, String> {
    let namespace = namespace(request.plan.plan());
    request.plan = request
        .plan
        .with_temporary_namespace(namespace)
        .map_err(|error| error.to_string())?;
    let kind = request.plan.journal_kind().to_owned();
    let bytes = serde_json::to_vec(&request.plan).map_err(|error| error.to_string())?;
    let steps =
        u32::try_from(request.plan.plan().steps.len()).map_err(|error| error.to_string())?;
    let job = lock(journal)
        .create_job(&kind, &bytes, steps, now_ms())
        .map_err(|error| error.to_string())?;
    let mut log = JournalLog::new(Arc::clone(journal), job, kind);
    log.overwrites = Some(request);
    Ok(log)
}

pub(super) fn finish(
    log: &JournalLog,
    state: JobState,
    ops: &dyn ProviderOps,
) -> Result<(), String> {
    let request = log
        .overwrites
        .as_ref()
        .ok_or("missing overwrite retention intent")?;
    let (current, job) = lock(&log.journal)
        .read_job(log.job)
        .map_err(|error| error.to_string())?;
    if current == state
        && !matches!(current, JobState::Running | JobState::RestorePending)
        && job.steps.iter().all(|step| step.detail.is_none())
    {
        return Ok(());
    }
    let keep =
        state == JobState::Failed && stranded(ops, &staged_saves(&log.journal, log.job, &job.plan));
    let (db, _) = crate::crash::open_db(&navo_store::StoreDirs::journal_in(&request.data))
        .map_err(|error| error.to_string())?;
    if let Err(error) = adopt(db, &log.journal, &job, (state, &request.data, ops, keep)) {
        let (current, _) = lock(&log.journal)
            .read_job(log.job)
            .map_err(|error| error.to_string())?;
        if current == state
            && !matches!(current, JobState::Running | JobState::RestorePending)
            && job.steps.iter().all(|step| step.detail.is_none())
        {
            return Ok(());
        }
        if matches!(current, JobState::Running | JobState::RestorePending) {
            lock(&log.journal)
                .finish(log.job, JobState::RestorePending)
                .map_err(|error| error.to_string())?;
        }
        return Err(navo_l10n::t!("job-retention-failed", error = error));
    }
    Ok(())
}

/// Settles a retained write at startup; `true` when a save left its staged bytes stranded.
pub(super) fn recover(
    journal: &SharedJournal,
    job: &UnfinishedJob,
    (data, ops): (&Path, &dyn ProviderOps),
) -> (JobState, bool) {
    let saves = staged_saves(journal, job.id, &job.plan);
    let keep = stranded(ops, &saves);
    let result = Db::open(&navo_store::StoreDirs::journal_in(data), false)
        .map_err(|error| error.to_string())
        .and_then(|(db, _)| {
            db.defer_checkpoint().map_err(|error| error.to_string())?;
            adopt(db, journal, job, (JobState::Failed, data, ops, keep))
        });
    match result {
        Ok(()) => (JobState::Failed, stranded(ops, &saves)),
        Err(error) => {
            tracing::warn!(job = job.id.0, %error, "overwrite retention recovery remains pending");
            (JobState::RestorePending, false)
        }
    }
}

pub(super) type StagedSave = (Location, Location, OverwriteProof);

pub(super) fn staged_saves(journal: &SharedJournal, job: JobId, plan: &[u8]) -> Vec<StagedSave> {
    let Ok(plan) = serde_json::from_slice::<RetainedWritePlan>(plan) else {
        return Vec::new();
    };
    if plan.plan().kind != OpKind::Save {
        return Vec::new();
    }
    let Ok(held) = held::held_of(journal, job) else {
        return Vec::new();
    };
    held.proved
        .into_iter()
        .filter_map(|(file, proof)| {
            Some((
                staged_for(plan.plan(), &file.original)?,
                file.original,
                proof,
            ))
        })
        .collect()
}

pub(super) fn staged_for(plan: &OperationPlan, target: &Location) -> Option<Location> {
    plan.steps.iter().find_map(|step| match step {
        Step::WriteFile {
            target: at, staged, ..
        } if at.same_place(target) => Some(staged.clone()),
        _ => None,
    })
}

/// `true` while a save's staged bytes may still be the only copy; only a definite answer clears it.
pub(super) fn stranded(ops: &dyn ProviderOps, saves: &[StagedSave]) -> bool {
    saves.iter().any(|(staged, target, proof)| {
        seen(ops, staged, proof.replacement_identity) != Seen::Other
            && seen(ops, target, proof.node.identity) != Seen::Same
    })
}

#[derive(PartialEq)]
enum Seen {
    Same,
    Other,
    Unknown,
}

fn seen(ops: &dyn ProviderOps, at: &Location, identity: navo_fs::FileIdentity) -> Seen {
    match ops.file_identity(at) {
        Ok(Some(found)) if found == identity => Seen::Same,
        Ok(Some(_)) => Seen::Other,
        Err(error) if error.kind == navo_core::FsErrorKind::NotFound => Seen::Other,
        _ => Seen::Unknown,
    }
}

fn adopt(
    db: Db,
    journal: &SharedJournal,
    job: &UnfinishedJob,
    (state, data, ops, keep_evidence): (JobState, &Path, &dyn ProviderOps, bool),
) -> Result<(), String> {
    let plan: RetainedWritePlan =
        serde_json::from_slice(&job.plan).map_err(|error| error.to_string())?;
    if job.kind != plan.journal_kind() {
        return Err("overwrite retention owner kind changed".to_owned());
    }
    super::temporary::cleanup(journal, (job, &plan), ops)?;
    let held = held::held_of(journal, job.id).map_err(|error| error.to_string())?;
    if held.unreadable != 0 || held.proved.len() != held.files.len() {
        return Err("overwrite retention evidence is missing or unreadable".to_owned());
    }
    let mut vault = Vault::new(db, data, ops).map_err(|error| error.to_string())?;
    vault
        .adopt_job(
            &AdoptJob {
                job: job.id,
                state,
                files: &held.proved,
                policy: plan.policy(),
                created_at: job.created_at,
                keep_evidence,
            },
            &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn namespace(plan: &navo_core::OperationPlan) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COPIES: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    loop {
        let value = format!(
            "{:x}-{now:x}-{:x}",
            std::process::id(),
            COPIES.fetch_add(1, Ordering::Relaxed)
        );
        let prefix = format!("{}-{value}-", navo_fs::TEMP_SUFFIX);
        if !plan.steps.iter().any(|step| collides(step, &prefix)) {
            return value;
        }
    }
}

fn collides(step: &navo_core::Step, prefix: &str) -> bool {
    use navo_core::Step;
    let matches = |path: &navo_core::Location| {
        path.name()
            .is_some_and(|name| name.to_text().to_ascii_lowercase().starts_with(prefix))
    };
    match step {
        Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
            matches(source) || matches(target)
        }
        Step::MakeFolder { target } | Step::MakeLink { at: target, .. } => matches(target),
        Step::WriteFile { target, staged, .. } => matches(target) || matches(staged),
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => matches(location),
    }
}

#[cfg(test)]
mod tests;
