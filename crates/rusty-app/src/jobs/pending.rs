use rusty_core::{Location, OperationPlan, Step};
use rusty_store::{JobId, JournalError, StepState, UnfinishedJob};

use super::{SharedJournal, lock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StillPending {
    pub job: JobId,
    pub left: Vec<Location>,
}

pub fn still_pending(journal: &SharedJournal) -> Result<Vec<StillPending>, JournalError> {
    let pending = lock(journal).restore_pending()?;
    Ok(pending
        .iter()
        .map(|job| StillPending {
            job: job.id,
            left: left_renamed(job),
        })
        .collect())
}

fn left_renamed(job: &UnfinishedJob) -> Vec<Location> {
    let Ok(plan) = serde_json::from_slice::<OperationPlan>(&job.plan) else {
        return Vec::new();
    };
    let moved: Vec<(&Location, &Location)> = job
        .steps
        .iter()
        .filter(|step| step.state == StepState::Done && !step.is_undone())
        .filter_map(
            |step| match plan.steps.get(usize::try_from(step.seq).ok()?)? {
                Step::Rename { source, target } => Some((source, target)),
                _ => None,
            },
        )
        .collect();
    moved
        .iter()
        .enumerate()
        .filter(|(at, (_, target))| !moved[at + 1..].iter().any(|(source, _)| source == target))
        .map(|(_, (_, target))| (*target).clone())
        .collect()
}
