use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

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
}

impl CacheDb {
    pub fn open(path: &Path) -> Result<(Self, CacheOpen), CacheError> {
        todo!()
    }

    pub fn open_in_memory() -> Result<Self, CacheError> {
        todo!()
    }

    pub fn set_budget(&mut self, table: &str, bytes: u64) {
        todo!()
    }

    pub fn get(&self, table: &str, key: &[u8]) -> Result<Option<Vec<u8>>, CacheError> {
        todo!()
    }

    pub fn put(&mut self, table: &str, key: &[u8], value: &[u8]) -> Result<(), CacheError> {
        todo!()
    }

    pub fn remove(&mut self, table: &str, key: &[u8]) -> Result<bool, CacheError> {
        todo!()
    }

    pub fn used_bytes(&self, table: &str) -> Result<u64, CacheError> {
        todo!()
    }

    pub fn clear(&mut self, table: &str) -> Result<(), CacheError> {
        todo!()
    }
}

#[cfg(test)]
mod tests;
