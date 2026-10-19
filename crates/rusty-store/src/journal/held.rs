use super::{JobId, JobJournal, JournalError};

impl JobJournal {
    pub fn record_held(
        &mut self,
        job: JobId,
        record: &[u8],
        created_at: i64,
    ) -> Result<(), JournalError> {
        self.db.connection().execute(
            "INSERT INTO undo (job_id, created_at, inverse) VALUES (?1, ?2, ?3)",
            rusqlite::params![job.0, created_at, record],
        )?;
        Ok(())
    }

    pub fn held(&self, job: JobId) -> Result<Vec<Vec<u8>>, JournalError> {
        let mut statement = self
            .db
            .connection()
            .prepare("SELECT inverse FROM undo WHERE job_id = ?1 ORDER BY id")?;
        let rows = statement
            .query_map([job.0], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn finished_holding(&self) -> Result<Vec<JobId>, JournalError> {
        let mut statement = self.db.connection().prepare(
            "SELECT DISTINCT job.id FROM job JOIN undo ON undo.job_id = job.id
             WHERE job.state != 'running' ORDER BY job.id",
        )?;
        let rows = statement
            .query_map([], |row| Ok(JobId(row.get(0)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn forget_held(&mut self, job: JobId) -> Result<u32, JournalError> {
        let deleted = self
            .db
            .connection()
            .execute("DELETE FROM undo WHERE job_id = ?1", [job.0])?;
        Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
    }
}
