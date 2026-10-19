use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::{FsError, FsErrorKind, OperationPlan};
use rusty_fs::{
    CancelToken, HeldFile, JobContext, JobLog, JobOutcome, Journaled, ProviderOps, StepOutcome,
    clean_up, roll_back,
};
use rusty_store::{Db, JobId, JobJournal, JobState, JournalError, StepState, UnfinishedJob};
use rusty_ui::browser::{LogFactory, OpLog};

mod held;

const KEEP_FINISHED: u32 = 50;

pub type SharedJournal = Arc<Mutex<JobJournal>>;

pub fn open(file: &Path) -> Result<SharedJournal, JournalError> {
    let (db, _) = Db::open(file, false)?;
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
        }) as Box<dyn OpLog>)
    })
}

struct JournalLog {
    journal: SharedJournal,
    job: JobId,
    kind: String,
    held: Vec<HeldFile>,
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
}

impl OpLog for JournalLog {
    fn finish(&mut self, outcome: &JobOutcome, ops: &dyn ProviderOps) {
        let state = match outcome {
            JobOutcome::Done => JobState::Done,
            JobOutcome::Cancelled => JobState::Cancelled,
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
        let _ = lock(&self.journal).finish(self.job, state);
        let _ = held::release(&self.journal, self.job, &self.held, ops);
    }
}

struct Quiet;

impl JobLog for Quiet {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

pub fn recover(journal: &SharedJournal, ops: &dyn ProviderOps) -> Result<u32, JournalError> {
    let unfinished = lock(journal).unfinished()?;
    let count = u32::try_from(unfinished.len()).unwrap_or(u32::MAX);
    for job in unfinished {
        let state = match held::held_of(journal, job.id)? {
            Some(held) => undo(&job, &held, ops),
            None => JobState::Failed,
        };
        lock(journal).finish(job.id, state)?;
    }
    held::release_finished(journal, ops)?;
    lock(journal).prune(KEEP_FINISHED)?;
    Ok(count)
}

fn undo(job: &UnfinishedJob, held: &[HeldFile], ops: &dyn ProviderOps) -> JobState {
    let Ok(plan) = serde_json::from_slice::<OperationPlan>(&job.plan) else {
        return JobState::Failed;
    };
    let done: Vec<u32> = job
        .steps
        .iter()
        .filter(|step| step.state == StepState::Done)
        .map(|step| step.seq)
        .collect();
    let _ = clean_up(&plan, &done, ops);
    let outcome = roll_back(
        &Journaled {
            plan: &plan,
            done: &done,
            held,
        },
        &mut JobContext {
            ops,
            log: &mut Quiet,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    match outcome {
        JobOutcome::Done => JobState::RolledBack,
        JobOutcome::Cancelled | JobOutcome::Failed { .. } => JobState::Failed,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod events_tests;

#[cfg(test)]
mod held_tests;
