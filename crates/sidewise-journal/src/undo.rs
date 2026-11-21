//! Journal undo (J5, NC148): reverts one committed job of this process with a new `Undo` job
//! whose items write each target's backup back, newest item first.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use navo_core::FileStamp;
use sidewise_fs::durable::Disk;

use crate::error::{JournalError, WriteError};
use crate::files::{backup_path, hash_file};
use crate::item::Expect;
use crate::journal::Journal;
use crate::record::{ItemMode, JobId, JobKind, Record, decode};

/// Why an undo wrote nothing, or was rolled back. Each variant says if a retry can help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoError {
    /// Permanent: the job's folder or log is gone.
    NotFound,
    /// Permanent: not a committed Copy, CopyMany or Sync job of this session.
    NotUndoable(String),
    /// Permanent: the log is damaged or does not describe whole items. Nothing was written.
    Damaged(String),
    /// Permanent until F2: the job has a Create or Remove item. Nothing was written.
    Unsupported,
    /// Permanent: this target no longer holds what the job wrote. Nothing is left written.
    Changed(PathBuf),
    /// Transient: the log or a target could not be read. Nothing was written.
    Io(String),
    /// A target write failed and the undo job was rolled back; `InUse` and `Full` are
    /// transient, the rest permanent.
    Write(WriteError),
    /// The undo job could not begin or commit; `InDoubt` is settled at the next start.
    Journal(JournalError),
}

impl fmt::Display for UndoError {
    /// A short English reason; the UI maps the variant to its own l10n id.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("undo: the job is gone from the journal"),
            Self::NotUndoable(why) | Self::Damaged(why) => write!(f, "undo: {why}"),
            Self::Unsupported => f.write_str("undo: the job creates or removes files"),
            Self::Changed(path) => write!(f, "undo: {} changed after the copy", path.display()),
            Self::Io(why) => write!(f, "undo: {why}"),
            Self::Write(error) => write!(f, "undo: {error}"),
            Self::Journal(error) => write!(f, "undo: {error}"),
        }
    }
}

impl std::error::Error for UndoError {}

/// One Overwrite item of the job to revert, read from its log.
struct Item {
    item: u32,
    target: PathBuf,
    /// What the job wrote: the target must still equal it.
    new: FileStamp,
    /// What the job replaced: its backup is written back.
    old: FileStamp,
}

impl Journal {
    /// Reverts the committed job `job` of this session (NC148): checks that every target
    /// still equals what the job wrote, then writes each backup back in a new `Undo` job,
    /// newest item first, with the old modified time. Returns the targets in item order.
    /// Fails with nothing written, or with the undo job rolled back (see `UndoError`).
    pub fn undo(&self, job: &JobId) -> Result<Vec<PathBuf>, UndoError> {
        let disk = self.inner.disk.as_ref();
        let dir = self.inner.root.join(job.as_str());
        let items = read_items(disk, &dir, job, &self.inner.owner)?;
        for item in &items {
            check_new(disk, item)?;
        }
        let mut undo = self
            .begin(JobKind::Undo, "Undo", Some(job.clone()))
            .map_err(UndoError::Journal)?;
        for item in items.iter().rev() {
            // The hash makes the overwrite refuse an edit that kept size and time before the
            // target is replaced, never after (NC160).
            let expect = Expect {
                size: Some(item.new.size),
                modified_ms: item.new.modified_ms,
                hash: Some(item.new.hash),
            };
            let backup = backup_path(&dir, item.item);
            let mut fill = |out: &mut dyn Write| stream_backup(disk, &backup, &item.old, out);
            undo.overwrite(&item.target, expect, item.old.modified_ms, &mut fill)
                .map_err(|error| match error {
                    WriteError::Changed => UndoError::Changed(item.target.clone()),
                    other => UndoError::Write(other),
                })?;
        }
        undo.commit().map_err(UndoError::Journal)?;
        Ok(items.into_iter().map(|item| item.target).collect())
    }
}

/// Reads the log of `id` in `dir` and returns its items in order. Fails unless the log is
/// whole, begins this job of `owner`, ends in `Commit`, is of an undoable kind, and holds
/// only whole Overwrite items with absolute targets.
fn read_items(
    disk: &dyn Disk,
    dir: &Path,
    id: &JobId,
    owner: &str,
) -> Result<Vec<Item>, UndoError> {
    let mut bytes = Vec::new();
    let read = disk
        .open_read(&dir.join("log"))
        .and_then(|mut log| log.read_to_end(&mut bytes));
    match read {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(UndoError::NotFound),
        Err(error) => return Err(UndoError::Io(format!("read job log: {error}"))),
        Ok(_) => {}
    }
    let log = decode(&bytes);
    if log.damaged || log.torn_tail {
        return Err(UndoError::Damaged("the job log is damaged".into()));
    }
    let not_undoable = |why: &str| Err(UndoError::NotUndoable(why.into()));
    match log.records.first() {
        Some(Record::Begin {
            job,
            kind,
            owner: by,
            ..
        }) if job == id => {
            if !matches!(kind, JobKind::Copy | JobKind::CopyMany | JobKind::Sync) {
                return not_undoable("an undo is not undone");
            }
            // Own jobs only: J4 sweeps only dead owners' jobs, so the backups stay put.
            if by != owner {
                return not_undoable("the job belongs to another session");
            }
        }
        _ => return Err(UndoError::Damaged("the log does not begin this job".into())),
    }
    match log.records.last() {
        Some(Record::Commit) => items_of(&log.records),
        _ if log.records.contains(&Record::Commit) => Err(UndoError::Damaged(
            "the log goes on after its commit".into(),
        )),
        _ => not_undoable("the job was not committed"),
    }
}

/// The items of a committed log, each with its `Staged`, `Backed` and `Done` records.
/// Fails with `Unsupported` on a Create or Remove item, `Damaged` on a partial one.
fn items_of(records: &[Record]) -> Result<Vec<Item>, UndoError> {
    let mut items: Vec<Item> = Vec::new();
    for record in records {
        let Record::Intent {
            item, target, mode, ..
        } = record
        else {
            continue;
        };
        if *mode != ItemMode::Overwrite {
            return Err(UndoError::Unsupported);
        }
        let damaged = || UndoError::Damaged(format!("item {item} is not whole"));
        let target = PathBuf::from(target);
        if !target.is_absolute() || items.iter().any(|seen| seen.item == *item) {
            return Err(damaged());
        }
        let new = records.iter().find_map(|record| match record {
            Record::Staged { item: found, new } if found == item => Some(*new),
            _ => None,
        });
        let old = records.iter().find_map(|record| match record {
            Record::Backed { item: found, old } if found == item => Some(*old),
            _ => None,
        });
        let done = records.contains(&Record::Done { item: *item });
        let (Some(new), Some(old), true) = (new, old, done) else {
            return Err(damaged());
        };
        items.push(Item {
            item: *item,
            target,
            new,
            old,
        });
    }
    Ok(items)
}

/// Fails with `Changed` unless the target still equals what the job wrote: size, modified
/// time and blake3 hash. A read error fails closed with `Io`.
fn check_new(disk: &dyn Disk, item: &Item) -> Result<(), UndoError> {
    let changed = || UndoError::Changed(item.target.clone());
    let io_error = |error: io::Error| UndoError::Io(format!("check target: {error}"));
    let meta = disk
        .meta(&item.target)
        .map_err(io_error)?
        .ok_or_else(changed)?;
    if meta.size != item.new.size || meta.modified_ms != item.new.modified_ms {
        return Err(changed());
    }
    match hash_file(disk, &item.target).map_err(io_error)? {
        Some(hash) if hash == item.new.hash => Ok(()),
        _ => Err(changed()),
    }
}

/// The fill of one undo item: streams the backup at `path` into `out` while hashing it.
/// Fails when it cannot be read, or when its bytes are not `old`, so a damaged backup is
/// never written back.
fn stream_backup(
    disk: &dyn Disk,
    path: &Path,
    old: &FileStamp,
    out: &mut dyn Write,
) -> Result<(), String> {
    let read_error = |error: io::Error| format!("read backup: {error}");
    let mut source = disk.open_read(path).map_err(read_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut len = 0_u64;
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        let read = match source.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(read_error(error)),
        };
        let chunk = buf.get(..read).unwrap_or(&buf);
        hasher.update(chunk);
        out.write_all(chunk)
            .map_err(|error| format!("write staged file: {error}"))?;
        len += read as u64;
    }
    if len == old.size && *hasher.finalize().as_bytes() == old.hash {
        Ok(())
    } else {
        Err("the backup does not match its hash".into())
    }
}

#[cfg(test)]
#[path = "undo_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "undo_crash_tests.rs"]
mod crash_tests;
