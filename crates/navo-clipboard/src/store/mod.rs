//! Clipboard history, saved items and folder notes database.

mod history;
mod ids;
mod notes;
mod safety;
mod saved;
mod wipe;

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

const VERSION: u32 = 3;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// Shorter than the 2 s the hooks wait, so a delete always answers before its caller gives up.
const CHECKPOINT_WAIT: Duration = Duration::from_millis(1500);
/// A table that exists while an upgrade's VACUUM and log truncate are still owed.
const CLEANUP_MARK: &str = "upgrade_cleanup";

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

/// Rebuilds `clips` and `saved` with AUTOINCREMENT so a deleted id is never issued again;
/// copying with explicit ids seeds `sqlite_sequence` with each table's highest id. The
/// cleanup mark makes every open retry the VACUUM until it has run.
const V3: &str = "
DROP INDEX clips_recent;
ALTER TABLE clips RENAME TO clips_v2;
CREATE TABLE clips (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
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
INSERT INTO clips (id, key, kind, body, paths, cut, source_app, first_ms, last_ms, uses)
    SELECT id, key, kind, body, paths, cut, source_app, first_ms, last_ms, uses FROM clips_v2;
DROP TABLE clips_v2;
CREATE INDEX clips_recent ON clips (last_ms, id);
ALTER TABLE saved RENAME TO saved_v2;
CREATE TABLE saved (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    user TEXT,
    sealed BLOB,
    scope TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
) STRICT;
INSERT INTO saved (id, kind, title, body, user, sealed, scope, created_ms, updated_ms)
    SELECT id, kind, title, body, user, sealed, scope, created_ms, updated_ms FROM saved_v2;
DROP TABLE saved_v2;
CREATE TABLE upgrade_cleanup (pending INTEGER PRIMARY KEY) STRICT;
";

const MIGRATIONS: [&str; 3] = [V1, V2, V3];

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
    /// A name for logs that carries no stored text.
    fn kind(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "sqlite",
            Self::CheckpointBusy => "checkpoint-busy",
            Self::Io(_) => "io",
            _ => "other",
        }
    }

    fn sqlite_code(&self) -> Option<ErrorCode> {
        match self {
            Self::Sqlite(error) => error.sqlite_error_code(),
            _ => None,
        }
    }

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
    /// A file from a newer version is refused and left untouched. A missing file with a
    /// `<name>.new` left by an interrupted Delete all opens that file.
    pub fn open(path: &Path, limits: Limits) -> Result<Self, ClipError> {
        wipe::adopt_staged(path);
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
        if let Err(error) = store.finish_cleanup() {
            tracing::warn!(
                kind = error.kind(),
                code = ?error.sqlite_code(),
                "clipboard upgrade cleanup failed; the next open retries it"
            );
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

    /// Runs the cleanup an upgrade left owed. secure_delete zeroed the dropped tables; VACUUM
    /// also drops text an older writer left in free pages, and the truncate moves the rebuilt
    /// file out of the log. The mark goes only after both, so a failure retries on next open.
    fn finish_cleanup(&self) -> Result<(), ClipError> {
        if !self.cleanup_owed()? {
            return Ok(());
        }
        #[cfg(test)]
        tests::fail_cleanup()?;
        self.conn.execute_batch("VACUUM")?;
        self.checkpoint()?;
        self.conn
            .execute_batch(&format!("DROP TABLE {CLEANUP_MARK}"))?;
        self.checkpoint()
    }

    fn cleanup_owed(&self) -> Result<bool, ClipError> {
        Ok(self.conn.query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [CLEANUP_MARK],
            |row| row.get(0),
        )?)
    }

    /// Copies the log into the database and empties it. Run after every delete or replace of
    /// stored text; [`ClipError::CheckpointBusy`] means a reader kept the log from emptying.
    fn checkpoint(&self) -> Result<(), ClipError> {
        self.conn.busy_timeout(CHECKPOINT_WAIT)?;
        let busy = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                row.get::<_, i32>(0)
            });
        self.conn.busy_timeout(BUSY_TIMEOUT)?;
        if busy? != 0 {
            return Err(ClipError::CheckpointBusy);
        }
        Ok(())
    }
}
