//! Clipboard history, saved items and folder notes database.

mod history;
mod notes;
mod safety;
mod saved;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::config::DbConfig;
use rusqlite::{Connection, ErrorCode, TransactionBehavior};

use crate::limits::Limits;

pub use history::Stored;
pub use notes::NoteListed;
pub use safety::Opened;
pub use saved::Listed;

const VERSION: u32 = 2;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const V1: &str = "
CREATE TABLE clips (
    id INTEGER PRIMARY KEY,
    key BLOB UNIQUE,
    kind INTEGER,
    body TEXT,
    paths TEXT,
    cut INTEGER,
    source_app TEXT,
    first_ms INTEGER,
    last_ms INTEGER,
    uses INTEGER
) STRICT;
CREATE INDEX clips_recent ON clips (last_ms, id);
";

const V2: &str = "
CREATE TABLE saved (
    id INTEGER PRIMARY KEY,
    kind INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    user TEXT,
    sealed BLOB,
    scope TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
) STRICT;
CREATE TABLE notes (
    key TEXT PRIMARY KEY,
    display TEXT NOT NULL,
    body TEXT NOT NULL,
    updated_ms INTEGER NOT NULL
) STRICT;
";

const MIGRATIONS: [&str; 2] = [V1, V2];

/// Why the clipboard store could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ClipError {
    #[error("clipboard database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("clipboard database version {found} is newer than supported version {supported}")]
    TooNew { found: u32, supported: u32 },
    #[error("clipboard row is malformed: {0}")]
    Malformed(&'static str),
    #[error("clipboard database checkpoint is busy")]
    CheckpointBusy,
    #[error("clipboard database failed its quick check")]
    Corrupt,
    #[error("the path is not the open clipboard database")]
    WrongPath,
    #[error("a secret must be stored as sealed bytes")]
    SecretNotSealed,
    #[error("a secret must not carry plain text")]
    PlainSecret,
    #[error("clipboard item of {bytes} bytes is over the {cap} byte cap")]
    TooLarge { bytes: usize, cap: usize },
    #[error("the saved item no longer exists")]
    Missing,
    #[error("the saved list is full at {cap} items")]
    TooManySaved { cap: usize },
    #[error("the note no longer exists")]
    NoteMissing,
    #[error("the target folder already has a note")]
    NoteExists,
    #[error("clipboard database file could not be moved or removed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the damaged clipboard database could not be fully put back after a failed reopen")]
    RestoreIncomplete { source: Box<ClipError> },
}

impl ClipError {
    fn is_corruption(&self) -> bool {
        match self {
            Self::Corrupt => true,
            Self::Sqlite(error) => matches!(
                error.sqlite_error_code(),
                Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
            ),
            _ => false,
        }
    }
}

/// The clipboard history database. One writer owns it.
pub struct ClipStore {
    conn: Connection,
    path: PathBuf,
    limits: Limits,
    opened: Opened,
}

impl ClipStore {
    /// Opens the store at `path`, creating and migrating the file when needed.
    /// A damaged file is moved aside and a fresh one opened; see [`ClipStore::opened`].
    /// A file from a newer version is refused and left untouched.
    pub fn open(path: &Path, limits: Limits) -> Result<Self, ClipError> {
        match Self::open_once(path, limits) {
            Err(error) if error.is_corruption() => safety::recover(path, limits),
            result => result,
        }
    }

    /// How the last open went: clean, or with the damaged file moved aside.
    pub fn opened(&self) -> &Opened {
        &self.opened
    }

    /// The path the store was opened with.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open_once(path: &Path, limits: Limits) -> Result<Self, ClipError> {
        let conn = Connection::open(path)?;
        // A file that fails its checks must close without a checkpoint writing into it.
        conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let mut store = Self {
            conn,
            path: path.to_path_buf(),
            limits,
            opened: Opened::Clean,
        };
        let version = store.version()?;
        if version > VERSION {
            return Err(ClipError::TooNew {
                found: version,
                supported: VERSION,
            });
        }
        store.quick_check()?;
        store.configure()?;
        if version < VERSION {
            store.migrate()?;
        }
        store
            .conn
            .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, false)?;
        Ok(store)
    }

    fn quick_check(&self) -> Result<(), ClipError> {
        let mut statement = self.conn.prepare("PRAGMA quick_check")?;
        let messages = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        match messages.as_slice() {
            [message] if message == "ok" => Ok(()),
            _ => Err(ClipError::Corrupt),
        }
    }

    fn version(&self) -> Result<u32, ClipError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    fn configure(&self) -> Result<(), ClipError> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA secure_delete = ON;
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(())
    }

    fn migrate(&mut self) -> Result<(), ClipError> {
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let done: u32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if done > VERSION {
            return Err(ClipError::TooNew {
                found: done,
                supported: VERSION,
            });
        }
        let done = usize::try_from(done).unwrap_or(usize::MAX);
        for step in MIGRATIONS.iter().skip(done) {
            transaction.execute_batch(step)?;
        }
        transaction.pragma_update(None, "user_version", VERSION)?;
        transaction.commit()?;
        Ok(())
    }

    /// Copies the log into the database and empties it. Run after every delete or replace of
    /// stored text; [`ClipError::CheckpointBusy`] means a reader kept the log from emptying.
    fn checkpoint(&self) -> Result<(), ClipError> {
        let busy: i32 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(ClipError::CheckpointBusy);
        }
        Ok(())
    }
}
