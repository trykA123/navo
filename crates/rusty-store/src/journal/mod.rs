use crate::db::{Db, DbError};

mod held;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
    RolledBack,
    RestorePending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalStep {
    pub seq: u32,
    pub state: StepState,
    pub detail: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnfinishedJob {
    pub id: JobId,
    pub kind: String,
    pub plan: Vec<u8>,
    pub created_at: i64,
    pub steps: Vec<JournalStep>,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("unknown job {0:?}")]
    UnknownJob(JobId),
    #[error("unknown step {seq} of job {job:?}")]
    UnknownStep { job: JobId, seq: u32 },
    #[error("bad state text {0:?}")]
    BadState(String),
}

pub struct JobJournal {
    db: Db,
}

impl JobJournal {
    pub fn new(db: Db) -> Result<Self, JournalError> {
        db.connection().execute_batch(
            "PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(Self { db })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P8 O3 contract"
    )]
    pub fn create_job(
        &mut self,
        kind: &str,
        plan: &[u8],
        steps: u32,
        created_at: i64,
    ) -> Result<JobId, JournalError> {
        let transaction = self.db.connection().unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO job (kind, state, created_at, plan) VALUES (?1, 'running', ?2, ?3)",
            rusqlite::params![kind, created_at, plan],
        )?;
        let id = transaction.last_insert_rowid();
        {
            let mut insert = transaction
                .prepare("INSERT INTO job_step (job_id, seq, state) VALUES (?1, ?2, 'pending')")?;
            for seq in 0..steps {
                insert.execute(rusqlite::params![id, seq])?;
            }
        }
        transaction.commit()?;
        Ok(JobId(id))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P8 O3 contract"
    )]
    pub fn mark_step(
        &mut self,
        job: JobId,
        seq: u32,
        state: StepState,
        detail: Option<&[u8]>,
    ) -> Result<(), JournalError> {
        let transaction = self.db.connection().unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE job_step SET state = ?3, detail = ?4 WHERE job_id = ?1 AND seq = ?2",
            rusqlite::params![job.0, seq, step_text(state), detail],
        )?;
        if changed == 0 {
            return Err(self.missing(job, seq)?);
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn finish(&mut self, job: JobId, state: JobState) -> Result<(), JournalError> {
        let changed = self.db.connection().execute(
            "UPDATE job SET state = ?2 WHERE id = ?1",
            rusqlite::params![job.0, job_text(state)],
        )?;
        if changed == 0 {
            return Err(JournalError::UnknownJob(job));
        }
        Ok(())
    }

    pub fn unfinished(&self) -> Result<Vec<UnfinishedJob>, JournalError> {
        self.jobs_in(JobState::Running)
    }

    pub fn restore_pending(&self) -> Result<Vec<UnfinishedJob>, JournalError> {
        self.jobs_in(JobState::RestorePending)
    }

    fn jobs_in(&self, state: JobState) -> Result<Vec<UnfinishedJob>, JournalError> {
        let mut jobs = self.db.connection().prepare(
            "SELECT id, kind, plan, created_at FROM job WHERE state = ?1
             ORDER BY created_at, id",
        )?;
        let rows = jobs
            .query_map([job_text(state)], |row| {
                Ok(UnfinishedJob {
                    id: JobId(row.get(0)?),
                    kind: row.get(1)?,
                    plan: row.get(2)?,
                    created_at: row.get(3)?,
                    steps: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|mut job| {
                job.steps = self.steps(job.id)?;
                Ok(job)
            })
            .collect()
    }

    pub fn prune(&mut self, keep_finished: u32) -> Result<u32, JournalError> {
        let deleted = self.db.connection().execute(
            "DELETE FROM job WHERE state != 'running'
             AND id NOT IN (SELECT job_id FROM undo WHERE job_id IS NOT NULL)
             AND id NOT IN (
                 SELECT id FROM job WHERE state != 'running'
                 ORDER BY created_at DESC, id DESC LIMIT ?1
             )",
            [keep_finished],
        )?;
        Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
    }

    fn steps(&self, job: JobId) -> Result<Vec<JournalStep>, JournalError> {
        let mut statement = self
            .db
            .connection()
            .prepare("SELECT seq, state, detail FROM job_step WHERE job_id = ?1 ORDER BY seq")?;
        let raw = statement
            .query_map([job.0], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(seq, state, detail)| {
                Ok(JournalStep {
                    seq,
                    state: parse_step(&state)?,
                    detail,
                })
            })
            .collect()
    }

    fn missing(&self, job: JobId, seq: u32) -> Result<JournalError, JournalError> {
        let exists: bool = self.db.connection().query_row(
            "SELECT EXISTS (SELECT 1 FROM job WHERE id = ?1)",
            [job.0],
            |row| row.get(0),
        )?;
        Ok(if exists {
            JournalError::UnknownStep { job, seq }
        } else {
            JournalError::UnknownJob(job)
        })
    }
}

fn step_text(state: StepState) -> &'static str {
    match state {
        StepState::Pending => "pending",
        StepState::Done => "done",
        StepState::Failed => "failed",
        StepState::Skipped => "skipped",
    }
}

fn parse_step(text: &str) -> Result<StepState, JournalError> {
    match text {
        "pending" => Ok(StepState::Pending),
        "done" => Ok(StepState::Done),
        "failed" => Ok(StepState::Failed),
        "skipped" => Ok(StepState::Skipped),
        other => Err(JournalError::BadState(other.to_owned())),
    }
}

fn job_text(state: JobState) -> &'static str {
    match state {
        JobState::Running => "running",
        JobState::Done => "done",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
        JobState::RolledBack => "rolled_back",
        JobState::RestorePending => "restore_pending",
    }
}

#[cfg(test)]
mod tests;
