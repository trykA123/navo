use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

mod budget;
mod connection;

const DEFAULT_BUDGET: u64 = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cache file error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheOpen {
    Opened,
    Created,
    Recreated { moved_to: PathBuf },
}

pub struct CacheDb {
    conn: Connection,
    budgets: HashMap<String, u64>,
    clock: Cell<i64>,
    budget_error: Cell<Option<rusqlite::Error>>,
}

impl CacheDb {
    pub fn open(path: &Path) -> Result<(Self, CacheOpen), CacheError> {
        let (conn, report) = connection::open(path)?;
        Ok((Self::from_connection(conn)?, report))
    }

    pub fn open_in_memory() -> Result<Self, CacheError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn set_budget(&mut self, table: &str, bytes: u64) {
        self.budgets.insert(table.to_owned(), bytes);
        if let Err(error) = budget::enforce(&mut self.conn, table, bytes) {
            self.budget_error.set(Some(error));
        }
    }

    pub fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, CacheError> {
        self.check_budget_error()?;
        let accessed = self.next_access()?;
        Ok(self
            .conn
            .query_row(
                "UPDATE blob SET accessed = ?3 WHERE tbl = ?1 AND key = ?2 RETURNING value",
                params![table, key, accessed],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn put(&mut self, table: &str, key: &[u8], value: &[u8]) -> Result<(), CacheError> {
        self.check_budget_error()?;
        let bytes = self.budgets.get(table).copied().unwrap_or(DEFAULT_BUDGET);
        let size = (key.len() as u64).saturating_add(value.len() as u64);
        let accessed = self.next_access()?;
        let transaction = self.conn.transaction()?;
        if size > bytes {
            transaction.execute(
                "DELETE FROM blob WHERE tbl = ?1 AND key = ?2",
                params![table, key],
            )?;
        } else {
            transaction.execute(
                "INSERT INTO blob (tbl, key, value, size, accessed)
                 VALUES (?1, ?2, ?3, length(?2) + length(?3), ?4)
                 ON CONFLICT(tbl, key) DO UPDATE SET
                     value = excluded.value, size = excluded.size, accessed = excluded.accessed",
                params![table, key, value, accessed],
            )?;
            budget::evict(&transaction, table, bytes)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn remove(&mut self, table: &str, key: &[u8]) -> Result<bool, CacheError> {
        self.check_budget_error()?;
        Ok(self.conn.execute(
            "DELETE FROM blob WHERE tbl = ?1 AND key = ?2",
            params![table, key],
        )? > 0)
    }

    pub fn used_bytes(&self, table: &str) -> Result<u64, CacheError> {
        self.check_budget_error()?;
        Ok(budget::used_bytes(&self.conn, table)?)
    }

    pub fn entries(&self, table: &str) -> Result<u64, CacheError> {
        self.check_budget_error()?;
        Ok(budget::entries(&self.conn, table)?)
    }

    pub fn clear(&mut self, table: &str) -> Result<(), CacheError> {
        self.check_budget_error()?;
        self.conn
            .execute("DELETE FROM blob WHERE tbl = ?1", [table])?;
        Ok(())
    }

    fn from_connection(conn: Connection) -> Result<Self, CacheError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = OFF;
             CREATE TABLE IF NOT EXISTS blob (
                 tbl TEXT, key BLOB, value BLOB, size INTEGER, accessed INTEGER,
                 PRIMARY KEY(tbl, key)
             );",
        )?;
        let clock = conn.query_row("SELECT COALESCE(MAX(accessed), 0) FROM blob", [], |row| {
            row.get(0)
        })?;
        Ok(Self {
            conn,
            budgets: HashMap::new(),
            clock: Cell::new(clock),
            budget_error: Cell::new(None),
        })
    }

    fn next_access(&self) -> Result<i64, CacheError> {
        let accessed = self
            .clock
            .get()
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("cache access clock exhausted"))?;
        self.clock.set(accessed);
        Ok(accessed)
    }

    fn check_budget_error(&self) -> Result<(), CacheError> {
        match self.budget_error.take() {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
