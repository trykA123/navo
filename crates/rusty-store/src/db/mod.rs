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

    pub fn open(path: &Path, unclean_exit: bool) -> Result<(Self, OpenReport), DbError> {
        let mut db = Self {
            conn: Connection::open(path)?,
        };
        let from_version = db.version()?;
        if from_version > Self::VERSION {
            return Err(DbError::TooNew {
                found: from_version,
                supported: Self::VERSION,
            });
        }
        if unclean_exit {
            db.check_integrity()?;
        }
        db.configure()?;
        db.migrate(from_version)?;
        let report = OpenReport {
            from_version,
            to_version: Self::VERSION,
            checked: unclean_exit,
        };
        Ok((db, report))
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let mut db = Self {
            conn: Connection::open_in_memory()?,
        };
        db.configure()?;
        db.migrate(0)?;
        Ok(db)
    }

    pub fn version(&self) -> Result<u32, DbError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn check_integrity(&self) -> Result<(), DbError> {
        let mut statement = self.conn.prepare("PRAGMA integrity_check")?;
        let messages = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        match messages.as_slice() {
            [message] if message == "ok" => Ok(()),
            _ => Err(DbError::Corrupt(messages.join("\n"))),
        }
    }

    fn configure(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(())
    }

    fn migrate(&mut self, from_version: u32) -> Result<(), DbError> {
        for (version, sql) in (1..=Self::VERSION)
            .zip(MIGRATIONS)
            .skip(from_version as usize)
        {
            let transaction = self.conn.transaction()?;
            transaction.execute_batch(sql)?;
            transaction.pragma_update(None, "user_version", version)?;
            transaction.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
