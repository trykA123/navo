use std::path::Path;

use rusqlite::Connection;

mod migrations;

use migrations::MIGRATIONS;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database version {found} is newer than the supported version {supported}")]
    TooNew { found: u32, supported: u32 },
    #[error("database integrity check failed: {0}")]
    Corrupt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    pub from_version: u32,
    pub to_version: u32,
    pub checked: bool,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub const VERSION: u32 = MIGRATIONS.len() as u32;

    pub fn open(_path: &Path, _unclean_exit: bool) -> Result<(Self, OpenReport), DbError> {
        todo!()
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        todo!()
    }

    pub fn version(&self) -> Result<u32, DbError> {
        todo!()
    }

    #[must_use]
    pub fn connection(&self) -> &Connection {
        todo!()
    }
}

#[cfg(test)]
mod tests;
