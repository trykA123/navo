use super::{JobId, JobJournal, JournalError};

pub struct StepEvidence<'a> {
    pub seq: u32,
    pub before: Option<&'a [u8]>,
    pub after: Option<&'a [u8]>,
}

impl JobJournal {
    pub fn replace_step_detail(
        &mut self,
        job: JobId,
        change: StepEvidence<'_>,
    ) -> Result<(), JournalError> {
        let changed = self.db.connection().execute(
            "UPDATE job_step SET detail = ?4 WHERE job_id = ?1 AND seq = ?2 AND detail IS ?3",
            rusqlite::params![job.0, change.seq, change.before, change.after],
        )?;
        if changed != 1 {
            return Err(JournalError::EvidenceChanged {
                job,
                seq: change.seq,
            });
        }
        Ok(())
    }
}
