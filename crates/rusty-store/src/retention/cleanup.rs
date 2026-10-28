use rusty_core::retention::{DAY_MS, RetentionError, RetentionId};

use super::{RetentionCatalog, RetentionStoreError};

const PRUNE_AGE_MS: i64 = 30 * DAY_MS;
const PRUNE_LIMIT: u32 = 1000;

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

    pub fn prune_terminal(&mut self, now: i64) -> Result<usize, RetentionStoreError> {
        if now < 0 {
            return Err(RetentionError::Invalid("negative prune time").into());
        }
        let tx = self.transaction()?;
        let pruned = tx.inner.execute(
            "DELETE FROM retained_entry WHERE id IN (SELECT e.id FROM retained_entry e
             WHERE e.state IN ('restored', 'purged', 'aborted') AND e.cleanup_pending = 0
             AND e.terminal_ms < ?1 AND NOT EXISTS (SELECT 1 FROM retained_entry l
                 WHERE l.job_id = e.job_id AND l.state NOT IN ('restored', 'purged', 'aborted'))
             ORDER BY e.terminal_ms, e.id LIMIT ?2)",
            rusqlite::params![now - PRUNE_AGE_MS, PRUNE_LIMIT],
        )?;
        tx.commit()?;
        Ok(pruned)
    }
}
