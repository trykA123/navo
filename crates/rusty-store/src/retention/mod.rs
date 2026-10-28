mod cleanup;
mod completion;
mod ownership;
mod protection;
mod rows;
mod transaction;

use rusqlite::{Transaction, TransactionBehavior};
use rusty_core::retention::{RetentionEntry, RetentionError, RetentionId, RetentionSummary};

use crate::{Db, DbError};

pub use completion::RetainedCompletion;
pub(crate) use rows::get;
pub(crate) use rows::validate;
pub use transaction::RetentionTransaction;

#[derive(Debug, thiserror::Error)]
pub enum RetentionStoreError {
    #[error("retention database: {0}")]
    Db(#[from] DbError),
    #[error("retention database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("retention record: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Invalid(#[from] RetentionError),
    #[error("retention entry {0:?} is missing")]
    Missing(RetentionId),
    #[error("retention entry {0:?} changed during the operation")]
    Stale(RetentionId),
    #[error("retention entry {0:?} belongs to an active or unverified job")]
    Pinned(RetentionId),
}

impl RetentionStoreError {
    pub fn is_corruption(&self) -> bool {
        match self {
            Self::Db(error) => error.is_corruption(),
            Self::Sqlite(error) => crate::db::is_corrupt_sqlite(error),
            _ => false,
        }
    }
}

pub struct RetentionCatalog {
    db: Db,
}

impl RetentionCatalog {
    pub fn new(db: Db) -> Result<Self, RetentionStoreError> {
        db.connection().pragma_update(None, "synchronous", "FULL")?;
        Ok(Self { db })
    }

    pub fn get(&self, id: RetentionId) -> Result<RetentionEntry, RetentionStoreError> {
        rows::get(self.db.connection(), id)
    }

    pub fn list_after(
        &self,
        after: i64,
        limit: u32,
    ) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
        rows::list(self.db.connection(), after, limit)
    }

    pub fn live_after(
        &self,
        after: i64,
        limit: u32,
    ) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
        rows::live(self.db.connection(), after, limit)
    }

    pub fn transaction(&mut self) -> Result<RetentionTransaction<'_>, RetentionStoreError> {
        Ok(RetentionTransaction {
            inner: Transaction::new_unchecked(
                self.db.connection(),
                TransactionBehavior::Immediate,
            )?,
        })
    }

    pub fn checked_transaction(
        &mut self,
        entry: &RetentionEntry,
    ) -> Result<RetentionTransaction<'_>, RetentionStoreError> {
        let transaction = self.transaction()?;
        if transaction.get(entry.id)? != *entry {
            return Err(RetentionStoreError::Stale(entry.id));
        }
        Ok(transaction)
    }

    pub fn summaries_after(
        &self,
        after: i64,
        limit: u32,
    ) -> Result<Vec<RetentionSummary>, RetentionStoreError> {
        rows::summaries(self.db.connection(), after, limit)
    }
}

#[cfg(test)]
mod tests;
