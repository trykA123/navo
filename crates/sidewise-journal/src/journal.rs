//! `Journal`: the open journal folder (`<dir>/v1/`, NC143) and this process's owner lock.
//! `open` recovers dead owners' jobs (J3); `begin` starts a job; the sweep (J4) builds on it.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sidewise_fs::durable::Disk;
use sidewise_fs::native::identity;

use crate::error::{JournalError, journal_io};
use crate::job::Job;
use crate::record::{JobId, JobKind, Record};
use crate::recover::{Recovered, recover_with};

/// The per-process job counter, the `seq` of every job id (NC143).
static SEQ: AtomicU64 = AtomicU64::new(0);

/// How many start times `open_raw` tries when an owner lock name is taken.
const OWNER_TRIES: i64 = 64;

/// What a journal and its jobs share.
pub(crate) struct Inner {
    /// `<dir>/v1`.
    pub(crate) root: PathBuf,
    pub(crate) disk: Arc<dyn Disk>,
    /// `<pid>-<start_ms>`, the stem of the owner lock.
    pub(crate) owner: String,
    /// Held open and locked for the life of the journal, so other processes see us alive.
    _lock: File,
}

/// The open journal of this process.
pub struct Journal {
    pub(crate) inner: Arc<Inner>,
}

impl Journal {
    /// Opens the journal in `dir` with no recovery: makes `dir`, `v1/` and `v1/owners/` as
    /// needed (each synced into its parent), then creates and locks this process's owner file.
    /// Fails when a folder step fails or the lock cannot be taken.
    pub fn open_raw(dir: &Path, disk: Arc<dyn Disk>) -> Result<Self, JournalError> {
        let root = dir.join("v1");
        let owners = root.join("owners");
        for folder in [dir, &root, &owners] {
            ensure_dir(disk.as_ref(), folder)?;
        }
        let (owner, lock) = lock_owner(&owners)?;
        disk.sync_dir(&owners)
            .map_err(|error| journal_io("sync owners folder", &error))?;
        let inner = Inner {
            root,
            disk,
            owner,
            _lock: lock,
        };
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    /// Opens the journal in `dir` (see `open_raw`), then rolls back every incomplete job of a
    /// dead owner (NC145), removes the lock files of dead owners whose jobs are all complete,
    /// and runs the retention sweep (NC141), whose counts are in `Recovered::swept`. Fails when
    /// the journal cannot open or its folders cannot be listed (writes stay off).
    pub fn open(dir: &Path, disk: Arc<dyn Disk>) -> Result<(Self, Recovered), JournalError> {
        let journal = Self::open_raw(dir, disk)?;
        let mut recovered = recover_with(&journal.inner, || {})?;
        recovered.swept = journal.sweep(now_ms().cast_unsigned())?;
        Ok((journal, recovered))
    }

    /// `open` with `between` run after recovery lists the jobs and before it claims any owner,
    /// so a test can let another process commit there. Linux only, like its race tests.
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn open_with(
        dir: &Path,
        disk: Arc<dyn Disk>,
        between: impl FnOnce(),
    ) -> Result<(Self, Recovered), JournalError> {
        let journal = Self::open_raw(dir, disk)?;
        let recovered = recover_with(&journal.inner, between)?;
        Ok((journal, recovered))
    }

    /// The `v1` folder that holds the job folders.
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// This process's owner stem, `<pid>-<start_ms>`.
    pub fn owner(&self) -> &str {
        &self.inner.owner
    }

    /// Starts a job: makes and syncs its folder, then writes and syncs its `Begin` line and the
    /// log's folder entry. Fails, leaving no job folder when it can, if any step fails.
    pub fn begin(
        &self,
        kind: JobKind,
        label: &str,
        reverts: Option<JobId>,
    ) -> Result<Job, JournalError> {
        let created_ms = now_ms().cast_unsigned();
        let id = JobId::new(
            created_ms,
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed),
        );
        let begin = Record::Begin {
            job: id.clone(),
            kind,
            label: label.to_owned(),
            created_ms,
            reverts,
            owner: self.inner.owner.clone(),
        };
        Job::start(Arc::clone(&self.inner), id, &begin)
    }
}

/// Creates `folder` when it is missing and syncs its parent. Fails when a step fails.
fn ensure_dir(disk: &dyn Disk, folder: &Path) -> Result<(), JournalError> {
    let found = disk
        .meta(folder)
        .map_err(|error| journal_io("read journal folder", &error))?;
    if found.is_some() {
        return Ok(());
    }
    disk.create_dir(folder)
        .map_err(|error| journal_io("make journal folder", &error))?;
    if let Some(parent) = folder.parent().filter(|p| !p.as_os_str().is_empty()) {
        disk.sync_dir(parent)
            .map_err(|error| journal_io("sync journal folder", &error))?;
    }
    Ok(())
}

/// Creates `owners/<pid>-<start_ms>.lock` with a name no one holds and takes its lock.
/// A taken name (a second journal in this process within one ms) moves `start_ms` on by one.
fn lock_owner(owners: &Path) -> Result<(String, File), JournalError> {
    lock_owner_with(owners, now_ms(), |_| {})
}

/// `lock_owner` from `start_ms`; `between` runs after the create and before the lock, so a
/// test can act as another process's recovery there.
pub(crate) fn lock_owner_with(
    owners: &Path,
    start_ms: i64,
    mut between: impl FnMut(&Path),
) -> Result<(String, File), JournalError> {
    for step in 0..OWNER_TRIES {
        let owner = format!("{}-{}", std::process::id(), start_ms + step);
        let path = owners.join(format!("{owner}.lock"));
        let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            opened => opened.map_err(|error| journal_io("make owner lock", &error))?,
        };
        between(&path);
        match file.try_lock() {
            Ok(()) => {}
            // Another process's recovery holds it and may delete it: take the next name.
            Err(TryLockError::WouldBlock) => continue,
            Err(TryLockError::Error(error)) => return Err(journal_io("lock owner file", &error)),
        }
        // A recovery may have deleted the file before our lock: then the lock guards nothing.
        if still_named(&file, &path)? {
            return Ok((owner, file));
        }
    }
    Err(JournalError::Io("no free owner lock name".into()))
}

/// True when `path` still names the open `file` (same volume and file id); false when the
/// name is gone or names another file. Fails closed when an identity is unreadable or unknown.
pub(crate) fn still_named(file: &File, path: &Path) -> Result<bool, JournalError> {
    let held = identity::from_file(file)
        .map_err(|error| journal_io("read owner lock identity", &error))?;
    let named = match identity::read(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        read => read.map_err(|error| journal_io("read owner lock name", &error))?,
    };
    match (held, named) {
        (Some(held), Some(named)) => Ok(held == named),
        _ => Err(JournalError::Io("owner lock identity unknown".into())),
    }
}

/// The wall clock in ms since the Unix epoch; 0 for a clock set before 1970.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|span| i64::try_from(span.as_millis()).ok())
        .unwrap_or(0)
}
