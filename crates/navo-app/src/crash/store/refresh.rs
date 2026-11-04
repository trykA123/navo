use std::path::Path;

use navo_core::Location;
use navo_store::{JobId, JobState, JournalError, UnfinishedJob};

use super::state::{Notice, RetryJob, State};
use super::{Error, details};
use crate::jobs::{RecoveryReport, SharedJournal, StillPending, lock};

pub(super) fn retry_jobs(
    pending: &[StillPending],
    journal: &SharedJournal,
) -> Result<Vec<RetryJob>, Error> {
    pending
        .iter()
        .map(|job| {
            let (_, saved) = lock(journal).read_job(job.job)?;
            Ok(RetryJob {
                id: job.job.0,
                fingerprint: fingerprint(&saved),
                paths: job
                    .left
                    .iter()
                    .take(4)
                    .map(|path| path.as_str().into())
                    .collect(),
            })
        })
        .collect()
}

pub(super) fn update(
    data: &Path,
    state: &mut State,
    report: &RecoveryReport,
    journal: &SharedJournal,
) -> Result<(), Error> {
    let mut changed = false;
    for notice in &mut state.notices {
        let before = notice.retry_jobs.len();
        let mut unresolved = Vec::new();
        for job in &notice.retry_jobs {
            if !restored(journal, job)? {
                unresolved.push(job.clone());
            }
        }
        notice.retry_jobs = unresolved;
        let resolved = before - notice.retry_jobs.len();
        let pending = with_attention(data, notice, report, journal);
        if resolved == 0 {
            // Only the details line is lost on failure; the notice itself is unchanged.
            if let Some(pending) = pending
                && let Err(error) = details::recovered(data, notice, &pending)
            {
                tracing::warn!(%error, "recovery details missed a needs-attention change");
            }
            continue;
        }
        notice.recovered_jobs += resolved;
        notice.unreadable_jobs = notice.unreadable_jobs.saturating_sub(resolved);
        details::recovered(data, notice, pending.as_deref().unwrap_or(&report.pending))?;
        changed = true;
    }
    if changed {
        state.save(data)?;
    }
    Ok(())
}

/// The pending list to save when a retry job of `notice` is Failed in the journal but the
/// details lack its attention flag, so the notice keeps that line once the job is pruned.
/// Reconciled from the journal, since a start may crash before the line is appended.
fn with_attention(
    data: &Path,
    notice: &Notice,
    report: &RecoveryReport,
    journal: &SharedJournal,
) -> Option<Vec<StillPending>> {
    let mut pending = report.pending.clone();
    let mut missing = false;
    for job in &notice.retry_jobs {
        if needs_attention(journal, job) != Some(true) || details::attention(data, notice, job.id) {
            continue;
        }
        missing = true;
        match pending.iter_mut().find(|known| known.job.0 == job.id) {
            Some(known) => known.attention = true,
            None => pending.push(StillPending {
                job: JobId(job.id),
                left: job
                    .paths
                    .iter()
                    .filter_map(|path| Location::new(path.as_str()).ok())
                    .collect(),
                attention: true,
            }),
        }
    }
    missing.then_some(pending)
}

/// A retained delete that settled as Failed is never retried; read from the journal, never saved.
/// `None` when the journal no longer has the job.
pub(super) fn needs_attention(journal: &SharedJournal, expected: &RetryJob) -> Option<bool> {
    match lock(journal).read_job(JobId(expected.id)) {
        Ok((state, job)) => Some(
            state == JobState::Failed
                && fingerprint(&job) == expected.fingerprint
                && job.kind == navo_core::retention::RETAINED_DELETE_KIND,
        ),
        Err(JournalError::UnknownJob(_)) => None,
        Err(_) => Some(false),
    }
}

fn restored(journal: &SharedJournal, expected: &RetryJob) -> Result<bool, Error> {
    match lock(journal).read_job(JobId(expected.id)) {
        Ok((state, job)) => Ok(fingerprint(&job) == expected.fingerprint
            && matches!(state, JobState::RolledBack | JobState::Undone)),
        Err(JournalError::UnknownJob(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn fingerprint(job: &UnfinishedJob) -> u64 {
    job.created_at
        .to_le_bytes()
        .iter()
        .chain(job.kind.as_bytes())
        .chain(&[0xff])
        .chain(&job.plan)
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}
