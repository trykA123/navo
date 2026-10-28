use std::collections::HashMap;
use std::path::Path;

use rusty_core::retention::RetentionPolicy;
use rusty_core::{Location, Step};
use rusty_fs::{CancelToken, ProviderOps};
use rusty_retention::{MaintenanceRequest, RetentionRun, Vault};
use rusty_store::JobState;

use super::JournalLog;

mod timer;

pub(crate) use timer::MaintenanceTimer;

pub(super) fn after(log: &JournalLog, state: JobState, ops: &dyn ProviderOps) {
    if log.global_lease.is_none()
        || !matches!(
            state,
            JobState::Done | JobState::Failed | JobState::Cancelled
        )
    {
        return;
    }
    let Some((data, parents)) = scope(log) else {
        return;
    };
    if let Err(error) = current(log).and_then(|policy| match policy {
        Some(policy) => maintain((data, policy, log.job), parents, ops),
        None => Ok(()),
    }) {
        tracing::warn!(job = log.job.0, %error, "retention maintenance deferred");
    }
}

fn current(log: &JournalLog) -> Result<Option<RetentionPolicy>, String> {
    let dirs = log
        .profile
        .as_ref()
        .ok_or("maintenance profile is unavailable")?;
    Ok(super::profile::settings(dirs)?.maintenance_policy())
}

fn scope(log: &JournalLog) -> Option<(&Path, Vec<Location>)> {
    if let Some(job) = &log.retained {
        let parents = job
            .plan
            .roots()
            .into_iter()
            .filter_map(|root| root.parent())
            .collect();
        return Some((&job.data, parents));
    }
    let job = log.overwrites.as_ref()?;
    let parents = job
        .plan
        .plan()
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::CopyFile { target, .. }
            | Step::Rename { target, .. }
            | Step::MakeFolder { target } => target.parent(),
            _ => None,
        })
        .collect();
    Some((&job.data, parents))
}

fn maintain(
    (data, policy, job): (&Path, RetentionPolicy, rusty_store::JobId),
    parents: Vec<Location>,
    ops: &dyn ProviderOps,
) -> Result<(), String> {
    let mut volumes = HashMap::new();
    for parent in parents {
        let identity = ops
            .file_identity(&parent)
            .map_err(|error| format!("{error:?}"))?
            .ok_or("maintenance parent has no stable identity")?;
        volumes.entry(identity.volume).or_insert(parent);
    }
    if volumes.is_empty() {
        return Ok(());
    }
    let (db, _) = crate::crash::open_db(&rusty_store::StoreDirs::journal_in(data))
        .map_err(|error| error.to_string())?;
    let mut vault = Vault::new(db, data, ops).map_err(|error| error.to_string())?;
    for at in volumes.values() {
        let request = MaintenanceRequest {
            at,
            policy,
            now: super::now_ms(),
            max_purges: 32,
        };
        match vault.maintain_job_volume(
            request,
            job,
            &mut RetentionRun::new(&CancelToken::new(), &mut |_| {}),
        ) {
            Ok(report) => tracing::info!(
                purged = report.purged.len(),
                quota_unmet = report.quota_unmet,
                low_space = report.low_space,
                budget_exhausted = report.budget_exhausted,
                cleanup_pending = report.cleanup_pending,
                "retention maintenance finished"
            ),
            Err(error) => tracing::warn!(%error, "retention volume maintenance deferred"),
        }
    }
    Ok(())
}
