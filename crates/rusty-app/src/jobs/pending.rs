use rusty_core::{Location, OperationPlan, Step};
use rusty_store::{JobId, JournalError, StepState, UnfinishedJob};

use super::SharedJournal;
use super::lock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StillPending {
    pub job: JobId,
    pub left: Vec<Location>,
}

#[cfg(test)]
pub fn still_pending(journal: &SharedJournal) -> Result<Vec<StillPending>, JournalError> {
    let pending = lock(journal).restore_pending()?;
    pending.iter().map(|job| details(journal, job)).collect()
}

pub(super) fn details(
    journal: &SharedJournal,
    job: &UnfinishedJob,
) -> Result<StillPending, JournalError> {
    let mut left = left_renamed(job);
    left.extend(super::temporary::paths(job));
    left.extend(lock(journal).retained_paths(job.id)?);
    if job.kind == rusty_core::retention::RETAINED_DELETE_KIND
        && let Ok(plan) =
            serde_json::from_slice::<rusty_core::retention::RetainedDeletePlan>(&job.plan)
    {
        left.extend(plan.roots());
    }
    if let Ok(plan) = serde_json::from_slice::<rusty_core::retention::RetainedWritePlan>(&job.plan)
    {
        for step in &plan.plan().steps {
            match step {
                Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
                    left.extend([source.clone(), target.clone()]);
                }
                _ => {}
            }
        }
    }
    let held = super::held::held_of(journal, job.id)?;
    for file in held.files {
        left.push(file.original);
        left.push(file.held);
    }
    left.extend(held.placed.into_iter().map(|file| file.target));
    let mut seen = std::collections::HashSet::new();
    left.retain(|path| seen.insert(path.clone()));
    Ok(StillPending { job: job.id, left })
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
