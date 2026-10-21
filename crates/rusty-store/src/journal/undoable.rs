use super::{JobId, JobJournal, JobState, JournalError, JournalStep};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedJob {
    pub id: JobId,
    pub plan: Vec<u8>,
    pub steps: Vec<JournalStep>,
    pub created_at: i64,
    pub restore_pending: bool,
}

impl JobJournal {
    pub fn last_undoable(&self, kind: &str) -> Result<Option<FinishedJob>, JournalError> {
        let row = self.db.connection().query_row(
            "SELECT id, plan, created_at, state = 'restore_pending' FROM job WHERE state IN ('done', 'restore_pending') AND kind = ?1
                 ORDER BY created_at DESC, id DESC LIMIT 1",
            [kind],
            |row| {
                Ok((
                    JobId(row.get(0)?),
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            },
        );
        match row {
            Ok((id, plan, created_at, restore_pending)) => Ok(Some(FinishedJob {
                id,
                plan,
                steps: self.steps(id)?,
                created_at,
                restore_pending,
            })),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn mark_job_undone(&mut self, job: JobId) -> Result<(), JournalError> {
        self.finish(job, JobState::Undone)
    }
}
