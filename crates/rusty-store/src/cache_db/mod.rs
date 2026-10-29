use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

mod budget;
mod connection;

const DEFAULT_BUDGET: u64 = 64 * 1024 * 1024;
const MAX_PENDING_ACCESSES: usize = 4096;
const ACCESS_BUSY_TIMEOUT: Duration = Duration::from_millis(100);

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
    accesses: RefCell<HashMap<(String, Vec<u8>), i64>>,
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
        let result = self
            .flush_accesses()
            .and_then(|()| budget::enforce(&mut self.conn, table, bytes));
        if let Err(error) = result {
            self.budget_error.set(Some(error));
        }
    }

    pub fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, CacheError> {
        self.check_budget_error()?;
        let accessed = self.next_access()?;
        let value = self
            .conn
            .query_row(
                "SELECT value FROM blob WHERE tbl = ?1 AND key = ?2",
                params![table, key],
                |row| row.get(0),
            )
            .optional()?;
        if value.is_some() {
            self.accesses
                .borrow_mut()
                .insert((table.to_owned(), key.to_vec()), accessed);
            if self.accesses.borrow().len() >= MAX_PENDING_ACCESSES
                && let Err(error) = self.flush_accesses_briefly()
            {
                tracing::debug!(%error, "cache access ages could not be saved at capacity");
                self.drop_oldest_accesses();
            }
        }
        Ok(value)
    }

    pub fn put(&mut self, table: &str, key: &[u8], value: &[u8]) -> Result<(), CacheError> {
        self.check_budget_error()?;
        let bytes = self.budgets.get(table).copied().unwrap_or(DEFAULT_BUDGET);
        let size = (key.len() as u64).saturating_add(value.len() as u64);
        let accessed = self.next_access()?;
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_accesses(&transaction, self.accesses.get_mut())?;
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
        self.accesses.get_mut().clear();
        Ok(())
    }

    pub fn remove(&mut self, table: &str, key: &[u8]) -> Result<bool, CacheError> {
        self.check_budget_error()?;
        let removed = self.conn.execute(
            "DELETE FROM blob WHERE tbl = ?1 AND key = ?2",
            params![table, key],
        )? > 0;
        self.accesses
            .get_mut()
            .remove(&(table.to_owned(), key.to_vec()));
        Ok(removed)
    }

    pub fn used_bytes(&self, table: &str) -> Result<u64, CacheError> {
        self.check_budget_error()?;
        Ok(budget::used_bytes(&self.conn, table)?)
    }

    pub fn usage(&self, table: &str) -> Result<(u64, u64), CacheError> {
        let bytes = budget::used_bytes(&self.conn, table)?;
        Ok((bytes, budget::entries(&self.conn, table)?))
    }

    pub fn clear(&mut self, table: &str) -> Result<(), CacheError> {
        self.check_budget_error()?;
        self.conn
            .execute("DELETE FROM blob WHERE tbl = ?1", [table])?;
        self.accesses
            .get_mut()
            .retain(|(pending_table, _), _| pending_table != table);
        Ok(())
    }

    fn from_connection(conn: Connection) -> Result<Self, CacheError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = OFF;
             CREATE TABLE IF NOT EXISTS blob (
                 tbl TEXT, key BLOB, value BLOB, size INTEGER, accessed INTEGER,
                 PRIMARY KEY(tbl, key)
             );
             CREATE INDEX IF NOT EXISTS blob_age ON blob (tbl, accessed, key, size);",
        )?;
        let clock = conn.query_row("SELECT COALESCE(MAX(accessed), 0) FROM blob", [], |row| {
            row.get(0)
        })?;
        Ok(Self {
            conn,
            budgets: HashMap::new(),
            clock: Cell::new(clock),
            budget_error: Cell::new(None),
            accesses: RefCell::new(HashMap::new()),
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

    fn flush_accesses(&self) -> Result<(), rusqlite::Error> {
        let mut pending = self.accesses.borrow_mut();
        if pending.is_empty() {
            return Ok(());
        }
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        write_accesses(&transaction, &pending)?;
        transaction.commit()?;
        pending.clear();
        Ok(())
    }

    fn flush_accesses_briefly(&self) -> Result<(), rusqlite::Error> {
        if self.accesses.borrow().is_empty() {
            return Ok(());
        }
        let timeout: i64 = self
            .conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
        let timeout = u64::try_from(timeout).unwrap_or(0);
        self.conn
            .busy_timeout(ACCESS_BUSY_TIMEOUT.min(Duration::from_millis(timeout)))?;
        let result = self.flush_accesses();
        let restored = self.conn.busy_timeout(Duration::from_millis(timeout));
        result?;
        restored
    }

    fn drop_oldest_accesses(&self) {
        let mut pending = self.accesses.borrow_mut();
        let mut ages: Vec<_> = pending.values().copied().collect();
        ages.sort_unstable();
        let keep_from = ages[ages.len() / 2];
        pending.retain(|_, age| *age >= keep_from);
    }

    fn check_budget_error(&self) -> Result<(), CacheError> {
        match self.budget_error.take() {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

impl Drop for CacheDb {
    fn drop(&mut self) {
        if let Err(error) = self.flush_accesses_briefly() {
            tracing::debug!(%error, "cache access ages could not be saved at exit");
        }
    }
}

fn write_accesses(
    conn: &Connection,
    pending: &HashMap<(String, Vec<u8>), i64>,
) -> Result<(), rusqlite::Error> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut statement = conn.prepare_cached(
        "UPDATE blob SET accessed = MAX(accessed, ?3) WHERE tbl = ?1 AND key = ?2",
    )?;
    for ((table, key), accessed) in pending {
        statement.execute(params![table, key, accessed])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
