use std::path::Path;

use rusty_store::{JobId, JobState, JournalError, UnfinishedJob};

use super::state::{RetryJob, State};
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
        if resolved == 0 {
            continue;
        }
        notice.recovered_jobs += resolved;
        notice.unreadable_jobs = notice.unreadable_jobs.saturating_sub(resolved);
        details::recovered(data, notice, &report.pending)?;
        changed = true;
    }
    if changed {
        state.save(data)?;
    }
    Ok(())
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
