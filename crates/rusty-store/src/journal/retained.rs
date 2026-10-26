use rusty_core::Location;
use rusty_core::retention::RetentionId;

use super::{JobId, JobJournal, JournalError};

impl JobJournal {
    pub fn retained_paths(&self, job: JobId) -> Result<Vec<Location>, JournalError> {
        let mut statement = self.db.connection().prepare(
            "SELECT id FROM retained_entry WHERE job_id = ?1 AND state NOT IN ('restored', 'purged', 'aborted') ORDER BY id",
        )?;
        let mut rows = statement.query([job.0])?;
        let mut paths = Vec::new();
        while let Some(row) = rows.next()? {
            let entry = crate::retention::get(self.db.connection(), RetentionId(row.get(0)?))?;
            paths.push(entry.record.source.clone());
            paths.push(entry.record.container.clone());
        }
        Ok(paths)
    }
}
