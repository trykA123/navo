use navo_core::retention::SAVE_RETAINED_KIND;
use navo_fs::ProviderOps;
use navo_store::{JobId, JobState, JournalError};

use super::super::{SharedJournal, StillPending, lock, overwrite, pending};

/// Lists failed saves that kept their proof rows while the staged bytes still wait.
pub(super) fn list(
    journal: &SharedJournal,
    ops: &dyn ProviderOps,
    report: &mut Vec<StillPending>,
) -> Result<(), JournalError> {
    let failed = lock(journal).failed_holding(SAVE_RETAINED_KIND, super::FAILED_SAVES_LISTED)?;
    if u32::try_from(failed.len()).unwrap_or(u32::MAX) >= super::FAILED_SAVES_LISTED {
        let all = lock(journal).count_failed_holding(SAVE_RETAINED_KIND)?;
        let unseen = all.saturating_sub(super::FAILED_SAVES_LISTED);
        tracing::info!(
            unseen,
            "older stranded saves were not checked at this start"
        );
    }
    for (id, plan) in failed {
        if report.iter().any(|listed| listed.job == id) {
            continue;
        }
        let saves = overwrite::staged_saves(journal, id, &plan);
        if saves.is_empty() {
            continue;
        }
        if overwrite::stranded(ops, &saves) {
            let (_, job) = lock(journal).read_job(id)?;
            report.push(pending::details(journal, &job)?);
        } else {
            tracing::info!(
                job = id.0,
                "stranded save no longer waits; its proof is released"
            );
            lock(journal).forget_held(id)?;
        }
    }
    Ok(())
}

/// Drops the proof rows of a stranded save once the user resolved it; `false` when `job` is not one.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the stranded save choices follow in a UI atom")
)]
pub(crate) fn resolve_stranded_save(
    journal: &SharedJournal,
    job: JobId,
) -> Result<bool, JournalError> {
    let mut journal = lock(journal);
    let (state, stored) = journal.read_job(job)?;
    if state != JobState::Failed || stored.kind != SAVE_RETAINED_KIND {
        return Ok(false);
    }
    journal.forget_held(job)?;
    Ok(true)
}
