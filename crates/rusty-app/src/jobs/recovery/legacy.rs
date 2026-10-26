use super::super::{SharedJournal, held, identity, lock};
use rusty_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan};
use rusty_fs::{
    CancelToken, FileIdentity, HeldFile, JobContext, JobLog, JobOutcome, Journaled, ProviderOps,
    StepOutcome, clean_up, roll_back,
};
use rusty_store::{JobId, JobState, JournalError, StepState, UnfinishedJob};

struct Recovery<'a> {
    journal: &'a SharedJournal,
    job: JobId,
    kept_beside: u32,
}

impl JobLog for Recovery<'_> {
    fn rename_identity(&self, seq: u32) -> Result<Option<FileIdentity>, FsError> {
        identity::read(self.journal, self.job, seq)
    }

    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome == StepOutcome::Failed {
            return Ok(());
        }
        lock(self.journal)
            .mark_undone(self.job, seq)
            .map_err(|_| FsError::new(FsErrorKind::Unknown))
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }

    fn kept_beside(&mut self, _: &HeldFile, _: &Location) {
        self.kept_beside += 1;
    }
}

fn left_to_undo(job: &UnfinishedJob) -> Vec<u32> {
    job.steps
        .iter()
        .filter(|step| step.state == StepState::Done && !step.is_undone())
        .map(|step| step.seq)
        .collect()
}

pub(super) fn undo(
    journal: &SharedJournal,
    job: &UnfinishedJob,
    ops: &dyn ProviderOps,
    experimental: bool,
) -> Result<JobState, JournalError> {
    let held = held::held_of(journal, job.id)?;
    let state = roll_back_job(journal, job, &held, (ops, experimental));
    Ok(match state {
        JobState::RolledBack if held.unreadable > 0 => JobState::RestorePending,
        other => other,
    })
}

fn roll_back_job(
    journal: &SharedJournal,
    job: &UnfinishedJob,
    held: &held::Held,
    (ops, experimental): (&dyn ProviderOps, bool),
) -> JobState {
    let Ok(plan) = serde_json::from_slice::<OperationPlan>(&job.plan) else {
        return unrestored(&held.files, false);
    };
    let renames = plan.kind == OpKind::Rename;
    if experimental && rusty_fs::check_mutation(&plan, ops, &CancelToken::new()).is_err() {
        return JobState::RestorePending;
    }
    let done = left_to_undo(job);
    let _ = clean_up(&plan, &done, ops);
    let mut log = Recovery {
        journal,
        job: job.id,
        kept_beside: 0,
    };
    let outcome = roll_back(
        &Journaled {
            plan: &plan,
            done: &done,
            held: &held.files,
            placed: &held.placed,
            held_unreadable: held.unreadable > 0,
            unsure: unsure_step(plan.kind, job),
        },
        &mut JobContext {
            ops,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    if log.kept_beside > 0 {
        tracing::warn!(
            job = job.id.0,
            files = log.kept_beside,
            "old files restored beside newer ones as \"(before copy)\""
        );
    }
    match outcome {
        JobOutcome::Done => JobState::RolledBack,
        JobOutcome::Cancelled | JobOutcome::Failed { .. } => unrestored(&held.files, renames),
    }
}

fn unsure_step(kind: OpKind, job: &UnfinishedJob) -> Option<u32> {
    let step = job
        .steps
        .iter()
        .find(|step| step.state != StepState::Done)?;
    match kind {
        OpKind::Rename => Some(step.seq),
        OpKind::Link if step.state == StepState::Pending => Some(step.seq),
        _ => None,
    }
}

fn unrestored(held: &[HeldFile], renames: bool) -> JobState {
    if held.is_empty() && !renames {
        JobState::Failed
    } else {
        JobState::RestorePending
    }
}
