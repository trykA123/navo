use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, FsErrorKind, Location, OperationPlan};
use rusty_fs::{
    CancelToken, HeldFile, JobContext, JobLog, JobOutcome, Journaled, PlacedFile, ProviderOps,
    StepOutcome, clean_up, roll_back,
};
use rusty_store::{JobId, JobJournal, JobState, JournalError, StepState, UnfinishedJob};
use rusty_ui::browser::{LogFactory, OpLog};

mod held;

const KEEP_FINISHED: u32 = 50;

pub type SharedJournal = Arc<Mutex<JobJournal>>;

pub fn open(file: &Path) -> Result<SharedJournal, JournalError> {
    let (db, _) = crate::crash::open_db(file)?;
    Ok(Arc::new(Mutex::new(JobJournal::new(db)?)))
}

fn lock(journal: &SharedJournal) -> MutexGuard<'_, JobJournal> {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

pub struct Instance {
    file: File,
    pub sole: bool,
}

impl Instance {
    pub fn share(self) -> Option<File> {
        if self.sole {
            self.file.unlock().ok()?;
            self.file.lock_shared().ok()?;
        }
        Some(self.file)
    }
}

pub fn claim(file: &Path) -> Option<Instance> {
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(file)
        .ok()?;
    if lock.try_lock().is_ok() {
        return Some(Instance {
            file: lock,
            sole: true,
        });
    }
    lock.lock_shared().ok()?;
    Some(Instance {
        file: lock,
        sole: false,
    })
}

pub fn log_factory(journal: SharedJournal) -> LogFactory {
    Arc::new(move |plan: &OperationPlan| {
        let bytes = serde_json::to_vec(plan).map_err(|error| error.to_string())?;
        let steps = u32::try_from(plan.steps.len()).map_err(|error| error.to_string())?;
        let kind = format!("{:?}", plan.kind).to_lowercase();
        let job = lock(&journal)
            .create_job(&kind, &bytes, steps, now_ms())
            .map_err(|error| error.to_string())?;
        Ok(Box::new(JournalLog {
            journal: Arc::clone(&journal),
            job,
            kind,
            held: Vec::new(),
            stranded: false,
        }) as Box<dyn OpLog>)
    })
}

struct JournalLog {
    journal: SharedJournal,
    job: JobId,
    kind: String,
    held: Vec<HeldFile>,
    stranded: bool,
}

impl JobLog for JournalLog {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        let state = match outcome {
            StepOutcome::Done => StepState::Done,
            StepOutcome::Skipped => StepState::Skipped,
            StepOutcome::Failed => StepState::Failed,
        };
        lock(&self.journal)
            .mark_step(self.job, seq, state, None)
            .map_err(|_| FsError::new(FsErrorKind::Unknown))
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        held::record(&self.journal, self.job, held)?;
        self.held.push(held.clone());
        Ok(true)
    }

    fn journal_placed(&mut self, placed: &PlacedFile) -> Result<(), FsError> {
        held::record_placed(&self.journal, self.job, placed)
    }

    fn held_stranded(&mut self, _: &HeldFile) {
        self.stranded = true;
    }
}

impl OpLog for JournalLog {
    fn finish(&mut self, outcome: &JobOutcome, ops: &dyn ProviderOps) {
        let state = match outcome {
            JobOutcome::Done => JobState::Done,
            JobOutcome::Cancelled => JobState::Cancelled,
            JobOutcome::Failed { .. } if self.stranded => JobState::RestorePending,
            JobOutcome::Failed { .. } => JobState::Failed,
        };
        if let JobOutcome::Failed { seq, error } = outcome {
            tracing::warn!(
                job = self.job.0,
                kind = self.kind,
                step = seq,
                error = ?error.kind,
                "job failed"
            );
        } else {
            tracing::info!(
                job = self.job.0,
                kind = self.kind,
                state = ?state,
                "job finished"
            );
        }
        if lock(&self.journal).finish(self.job, state).is_ok() && !self.stranded {
            let _ = held::release(&self.journal, self.job, &self.held, ops);
        }
    }
}

struct Recovery<'a> {
    journal: &'a SharedJournal,
    job: JobId,
    kept_beside: u32,
}

impl JobLog for Recovery<'_> {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
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

pub fn recover(journal: &SharedJournal, ops: &dyn ProviderOps) -> Result<u32, JournalError> {
    let unfinished = lock(journal).unfinished()?;
    let count = u32::try_from(unfinished.len()).unwrap_or(u32::MAX);
    for job in unfinished.iter().rev() {
        let state = undo(journal, job, ops)?;
        lock(journal).finish(job.id, state)?;
    }
    let pending = lock(journal).restore_pending()?;
    for job in pending.iter().rev() {
        let state = undo(journal, job, ops)?;
        lock(journal).finish(job.id, state)?;
    }
    held::release_finished(journal, ops)?;
    lock(journal).prune(KEEP_FINISHED)?;
    Ok(count)
}

fn left_to_undo(job: &UnfinishedJob) -> Vec<u32> {
    job.steps
        .iter()
        .filter(|step| step.state == StepState::Done && !step.is_undone())
        .map(|step| step.seq)
        .collect()
}

fn undo(
    journal: &SharedJournal,
    job: &UnfinishedJob,
    ops: &dyn ProviderOps,
) -> Result<JobState, JournalError> {
    let held = held::held_of(journal, job.id)?;
    let state = roll_back_job(journal, job, &held, ops);
    Ok(match state {
        JobState::RolledBack if held.unreadable > 0 => JobState::RestorePending,
        other => other,
    })
}

fn roll_back_job(
    journal: &SharedJournal,
    job: &UnfinishedJob,
    held: &held::Held,
    ops: &dyn ProviderOps,
) -> JobState {
    let Ok(plan) = serde_json::from_slice::<OperationPlan>(&job.plan) else {
        return unrestored(&held.files);
    };
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
        JobOutcome::Cancelled | JobOutcome::Failed { .. } => unrestored(&held.files),
    }
}

fn unrestored(held: &[HeldFile]) -> JobState {
    if held.is_empty() {
        JobState::Failed
    } else {
        JobState::RestorePending
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod events_tests;

#[cfg(test)]
mod held_tests;

#[cfg(test)]
mod move_over_tests;

#[cfg(test)]
mod undo_guard_tests;

#[cfg(test)]
mod undo_retry_tests;

#[cfg(test)]
mod parallel_tests;
