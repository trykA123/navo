use crate::db::{Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
    RolledBack,
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
        let _ = db;
        todo!()
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
        let _ = (kind, plan, steps, created_at);
        todo!()
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
        let _ = (job, seq, state, detail);
        todo!()
    }

    pub fn finish(&mut self, job: JobId, state: JobState) -> Result<(), JournalError> {
        let _ = (job, state);
        todo!()
    }

    pub fn unfinished(&self) -> Result<Vec<UnfinishedJob>, JournalError> {
        todo!()
    }

    pub fn prune(&mut self, keep_finished: u32) -> Result<u32, JournalError> {
        let _ = keep_finished;
        todo!()
    }
}

#[cfg(test)]
mod tests;
