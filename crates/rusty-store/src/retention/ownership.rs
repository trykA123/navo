use rusty_core::retention::{MAX_NODES, RetentionError, RetentionId};

use super::{RetentionCatalog, RetentionStoreError};
use crate::JobId;

impl RetentionCatalog {
    pub fn ids_for_job(&self, job: JobId) -> Result<Vec<RetentionId>, RetentionStoreError> {
        let mut query = self
            .db
            .connection()
            .prepare("SELECT id FROM retained_entry WHERE job_id = ?1 LIMIT ?2")?;
        let ids = query
            .query_map(
                rusqlite::params![
                    job.0,
                    i64::try_from(MAX_NODES + 1)
                        .map_err(|_| RetentionError::Invalid("invalid inventory limit"))?
                ],
                |row| row.get(0).map(RetentionId),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() > MAX_NODES {
            return Err(RetentionError::Invalid("job retention inventory is too large").into());
        }
        Ok(ids)
    }
}
