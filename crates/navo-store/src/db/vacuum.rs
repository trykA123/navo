use std::time::Duration;

use super::{BUSY_TIMEOUT, Db, DbError};

const INCREMENTAL: i64 = 2;

impl Db {
    /// True when freed pages can be returned with `incremental_vacuum`.
    pub fn is_incremental_vacuum(&self) -> Result<bool, DbError> {
        let mode: i64 = self
            .conn
            .pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
        Ok(mode == INCREMENTAL)
    }

    pub(crate) fn size_bytes(&self) -> Result<u64, DbError> {
        let pages = self.count_pragma("page_count")?;
        Ok(pages.saturating_mul(self.count_pragma("page_size")?))
    }

    // SQLite rebuilds the file atomically; an interrupted VACUUM leaves the old database.
    pub(crate) fn vacuum_incremental(&self) -> Result<(), DbError> {
        self.conn
            .execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")?;
        if !self.is_incremental_vacuum()? {
            return Err(DbError::VacuumIncomplete);
        }
        Ok(())
    }

    pub(crate) fn reclaim_free_pages(&self, pages: u32) -> Result<u64, DbError> {
        if !self.is_incremental_vacuum()? {
            return Ok(0);
        }
        self.conn
            .execute_batch(&format!("PRAGMA incremental_vacuum({pages})"))?;
        self.count_pragma("freelist_count")
    }

    pub(super) fn count_pragma(&self, name: &str) -> Result<u64, DbError> {
        let value: i64 = self.conn.pragma_query_value(None, name, |row| row.get(0))?;
        Ok(u64::try_from(value).unwrap_or(0))
    }

    /// Makes busy databases fail at once until the guard drops, which restores the normal wait.
    pub(crate) fn no_wait(&self) -> Result<NoWait<'_>, DbError> {
        self.conn.busy_timeout(Duration::ZERO)?;
        Ok(NoWait(&self.conn))
    }

    pub(crate) fn try_truncate_wal(&self) -> Result<bool, DbError> {
        self.conn.busy_timeout(Duration::ZERO)?;
        let checked = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                row.get::<_, i32>(0)
            });
        self.conn.busy_timeout(BUSY_TIMEOUT)?;
        match checked {
            Ok(busy) => Ok(busy == 0),
            Err(error) if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy) => {
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }
}

pub(crate) struct NoWait<'a>(&'a rusqlite::Connection);

impl Drop for NoWait<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.0.busy_timeout(BUSY_TIMEOUT) {
            tracing::warn!(%error, "database busy timeout not restored");
        }
    }
}
