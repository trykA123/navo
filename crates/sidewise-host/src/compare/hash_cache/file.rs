//! The file-backed hash cache: one process per profile writes, the others only read.
//! Opens, locks, replays and appends to `hashes.v1`; the compare engine uses it through `HashCache`.

use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_compare::model::{ContentKey, HashCache};

pub use super::compact::Limits;
use super::compact::{keep_newest, needs_rewrite, rewrite};
use super::record::{self, Digest, MAGIC, RECORD_LEN};

/// How a store keeps its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Holds the profile lock and owns the file.
    Writer,
    /// Another process writes; this one read the file at open and keeps puts in memory.
    Reader,
    /// No file at all.
    Memory,
}

/// Content hashes keyed by file digest, kept in memory and, for the writer, in a file.
pub struct FileHashes {
    path: Option<PathBuf>,
    /// Rows at which `put` stops adding new keys.
    ceiling_rows: usize,
    /// True when another process held the profile lock at open: this store cannot clear.
    contended: bool,
    state: Mutex<State>,
}

/// Why `clear_all` did not clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearError {
    /// Another process holds the cache; nothing changed.
    Busy,
    /// The file could not be removed (the text); the rows are kept.
    Io(String),
}

impl fmt::Display for ClearError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("another Sidewise window is using the compare cache"),
            Self::Io(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for ClearError {}

/// One cached hash and its write order (`seq`, rising), used when rewriting.
pub(super) struct Row {
    pub(super) hash: Digest,
    pub(super) seq: u64,
}

struct State {
    rows: HashMap<Digest, Row>,
    next_seq: u64,
    writer: Option<Writer>,
    /// Encoded records not yet appended to the file (writer only).
    pending: Vec<u8>,
}

/// Present only in the writer: the held lock on `<path>.lock` and the append handle,
/// opened on the first flush.
struct Writer {
    _lock: File,
    log: Option<File>,
}

/// Buffered records that trigger an append.
const FLUSH_RECORDS: usize = 1024;
/// A file modified less than this many ms ago may still be written (git's "racy clean").
const RACY_MS: i64 = 10_000;

impl FileHashes {
    /// Opens the cache at `path` with the default limits; see `open_with`.
    pub fn open(path: &Path) -> Self {
        Self::open_with(path, Limits::DEFAULT)
    }

    /// Opens the cache at `path`, keeping at most `limits.cap_rows` rows. Never fails:
    /// problems are logged and the store falls back to reader or memory mode.
    pub fn open_with(path: &Path, limits: Limits) -> Self {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            && let Err(error) = fs::create_dir_all(parent)
        {
            tracing::warn!(%error, path = %parent.display(), "hash cache folder: memory only");
            return Self::memory();
        }
        let mut writer = take_lock(path);
        let contended = writer.is_none();
        let replay = read_log(path);
        let damaged = !replay.header_ok || replay.bad > 0 || replay.torn;
        let records = replay.rows.len() + replay.bad;
        let (mut rows, next_seq) = index_rows(replay.rows);
        let stale = needs_rewrite(records, rows.len(), limits);
        keep_newest(&mut rows, limits.cap_rows);
        if writer.is_some()
            && (damaged || stale)
            && let Err(error) = rewrite(path, &rows)
        {
            tracing::warn!(%error, path = %path.display(), "hash cache rewrite failed; not writing");
            writer = None;
        }
        Self {
            path: Some(path.to_path_buf()),
            ceiling_rows: limits.ceiling_rows,
            contended,
            state: Mutex::new(State {
                rows,
                next_seq,
                writer,
                pending: Vec::new(),
            }),
        }
    }

    /// A store without a file.
    pub fn memory() -> Self {
        Self {
            path: None,
            ceiling_rows: Limits::DEFAULT.ceiling_rows,
            contended: false,
            state: Mutex::new(State {
                rows: HashMap::new(),
                next_seq: 0,
                writer: None,
                pending: Vec::new(),
            }),
        }
    }

    /// Whether this store writes the file, only reads it, or has none.
    pub fn mode(&self) -> CacheMode {
        match (&self.path, &self.state().writer) {
            (None, _) => CacheMode::Memory,
            (Some(_), Some(_)) => CacheMode::Writer,
            (Some(_), None) => CacheMode::Reader,
        }
    }

    /// How many hashes the store holds.
    pub fn len(&self) -> usize {
        self.state().rows.len()
    }

    /// True when the store holds no hash.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops every hash and returns how many there were. A writer also removes the file, and
    /// later puts start a fresh one. Memory-only stores (no file, or a writer whose open or
    /// append failed) clear memory only. Fails with `Busy` when another process holds the
    /// cache, and with `Io` when the file cannot be removed (rows are kept then).
    pub fn clear_all(&self) -> Result<usize, ClearError> {
        if self.contended {
            return Err(ClearError::Busy);
        }
        let mut state = self.state();
        if let (Some(path), Some(writer)) = (&self.path, state.writer.as_mut()) {
            writer.log = None;
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(ClearError::Io(error.to_string())),
            }
        }
        state.pending.clear();
        let count = state.rows.len();
        state.rows.clear();
        Ok(count)
    }

    /// Locks the state; a panic in another thread leaves plain data, so poison is ignored.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Appends the pending records to the file; only a writer with pending bytes acts.
    /// A failed append is logged and turns this store memory-only for the rest of the run.
    fn flush_locked(&self, state: &mut State) {
        let State {
            writer, pending, ..
        } = state;
        let (Some(path), Some(open)) = (&self.path, writer.as_mut()) else {
            return;
        };
        if pending.is_empty() {
            return;
        }
        if let Err(error) = append(path, open, pending) {
            tracing::warn!(%error, path = %path.display(), "hash cache append failed; memory only");
            *writer = None;
        }
        pending.clear();
    }
}

/// Reads and replays the log at `path`; a missing file is an empty log, and an unreadable
/// one (logged) replays as a bad header, so a writer rewrites it.
fn read_log(path: &Path) -> record::Replay {
    match fs::read(path) {
        Ok(bytes) => record::replay(&bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => record::replay(&[]),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "hash cache unreadable");
            record::Replay::default()
        }
    }
}

/// Builds the row map from replayed records in file order; a later record for a key wins
/// and takes the later `seq`. Returns the map and the next free `seq`.
fn index_rows(records: Vec<(Digest, Digest)>) -> (HashMap<Digest, Row>, u64) {
    let mut rows = HashMap::with_capacity(records.len());
    let mut next_seq = 0;
    for (key, hash) in records {
        rows.insert(
            key,
            Row {
                hash,
                seq: next_seq,
            },
        );
        next_seq += 1;
    }
    (rows, next_seq)
}

/// Appends `bytes` to the log at `path`, opening it on first use and writing `MAGIC` into
/// an empty file first. Fails on any I/O error; a partial write leaves a torn tail.
fn append(path: &Path, writer: &mut Writer, bytes: &[u8]) -> io::Result<()> {
    let log = match writer.log.take() {
        Some(log) => log,
        None => OpenOptions::new().append(true).create(true).open(path)?,
    };
    let log = writer.log.insert(log);
    if log.metadata()?.len() == 0 {
        log.write_all(&MAGIC)?;
    }
    log.write_all(bytes)
}

/// True when `modified` (ms since the epoch) is at least `RACY_MS` before now and not in
/// the future. A missing time or an unreadable clock is never settled.
fn settled(modified: Option<i64>) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok());
    matches!((modified, now), (Some(modified), Some(now)) if modified <= now - RACY_MS)
}

/// Takes the profile lock on `<path>.lock`; `None` means another process writes, or the lock
/// could not be taken (logged), so this store only reads.
fn take_lock(path: &Path) -> Option<Writer> {
    let lock_path = with_suffix(path, ".lock");
    let result = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(TryLockError::Error)
        .and_then(|file| file.try_lock().map(|()| file));
    match result {
        Ok(file) => Some(Writer {
            _lock: file,
            log: None,
        }),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Error(error)) => {
            tracing::warn!(%error, path = %lock_path.display(), "hash cache lock failed; reading only");
            None
        }
    }
}

/// Returns `path` with `suffix` appended to its file name (`hashes.v1` + `.lock`).
pub(super) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

impl HashCache for FileHashes {
    fn get(&self, key: &ContentKey) -> Option<[u8; 32]> {
        let digest = record::key_digest(key)?;
        self.state().rows.get(&digest).map(|row| row.hash)
    }

    /// Stores `hash` for a settled file; a writer buffers the record and appends at
    /// `FLUSH_RECORDS`. An untrusted key, a racy time, an unchanged row or a new key at
    /// the ceiling stores nothing.
    fn put(&self, key: &ContentKey, hash: [u8; 32]) {
        let Some(digest) = record::key_digest(key) else {
            return;
        };
        if !settled(key.modified) {
            return;
        }
        let mut state = self.state();
        match state.rows.get(&digest) {
            Some(row) if row.hash == hash => return,
            None if state.rows.len() >= self.ceiling_rows => return,
            _ => {}
        }
        let seq = state.next_seq;
        state.next_seq += 1;
        state.rows.insert(digest, Row { hash, seq });
        if state.writer.is_some() {
            state
                .pending
                .extend_from_slice(&record::encode(&digest, &hash));
            if state.pending.len() >= FLUSH_RECORDS * RECORD_LEN {
                self.flush_locked(&mut state);
            }
        }
    }

    fn flush(&self) {
        let mut state = self.state();
        self.flush_locked(&mut state);
    }
}

impl Drop for FileHashes {
    /// Appends any buffered records before the lock is released.
    fn drop(&mut self) {
        self.flush();
    }
}
