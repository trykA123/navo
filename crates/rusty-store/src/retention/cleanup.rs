use rusty_core::retention::{RetentionError, RetentionId};

use super::{RetentionCatalog, RetentionStoreError};

impl RetentionCatalog {
    pub fn pending_cleanup(
        &self,
        volume: u64,
        limit: u32,
    ) -> Result<Vec<RetentionId>, RetentionStoreError> {
        if !(1..=1000).contains(&limit) {
            return Err(RetentionError::Invalid("invalid cleanup page bounds").into());
        }
        let mut statement = self.db.connection().prepare(
            "SELECT id FROM retained_entry WHERE cleanup_pending = 1
             AND state IN ('restored', 'purged', 'aborted') AND volume_key = ?1
             ORDER BY cleanup_round, id LIMIT ?2",
        )?;
        let ids = statement
            .query_map(rusqlite::params![volume.to_string(), limit], |row| {
                Ok(RetentionId(row.get(0)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    pub fn begin_cleanup_attempt(&mut self, id: RetentionId) -> Result<(), RetentionStoreError> {
        let tx = self.transaction()?;
        if !tx.get(id)?.state.is_terminal() {
            return Err(RetentionError::Invalid("only terminal cleanup can be attempted").into());
        }
        tx.inner.execute(
            "UPDATE retained_entry SET cleanup_round = cleanup_round + 1 WHERE id = ?1",
            [id.0],
        )?;
        tx.commit()
    }

    pub fn acknowledge_cleanup(&mut self, id: RetentionId) -> Result<(), RetentionStoreError> {
        let tx = self.transaction()?;
        if !tx.get(id)?.state.is_terminal() {
            return Err(
                RetentionError::Invalid("only terminal cleanup can be acknowledged").into(),
            );
        }
        tx.inner.execute(
            "UPDATE retained_entry SET cleanup_pending = 0 WHERE id = ?1",
            [id.0],
        )?;
        tx.commit()
    }
}
