//! The compare spool folder (V34): one locked folder per process, spool files that delete
//! themselves, the copy into them, and the start-up sweep of folders whose process is gone.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_compare::model::{CancelToken, Reason};

const COPY_CHUNK: usize = 1024 * 1024;
const OWNER_LOCK: &str = "owner.lock";
/// Owners hold this shared while they create and lock their folder; a sweep holds it exclusive.
const GATE: &str = "sweep.lock";

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
static START_NANOS: OnceLock<u128> = OnceLock::new();
static OWNERS: OnceLock<Mutex<HashMap<PathBuf, Weak<Owner>>>> = OnceLock::new();

/// The process's spool folder and its owner lock.
pub(super) struct Owner {
    dir: PathBuf,
    _lock: File,
}

impl Owner {
    /// Create `<pid>-<start-nanos>` and lock its `owner.lock` under the shared gate, so a sweep
    /// never sees the folder before it is locked. Fails when the gate or lock cannot be taken.
    fn open(base: &Path) -> io::Result<Self> {
        let gate = open_gate(base)?;
        gate.lock_shared()?;
        let nanos = START_NANOS.get_or_init(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        });
        let dir = base.join(format!("{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(OWNER_LOCK))?;
        lock.try_lock().map_err(io::Error::from)?;
        drop(gate);
        Ok(Self { dir, _lock: lock })
    }
}

impl Drop for Owner {
    /// Remove the process folder once its sources are gone.
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.dir) {
            tracing::warn!(%error, path = %self.dir.display(), "compare spool folder could not be removed");
        }
    }
}

/// One spool file, deleted when the last source holding it drops.
pub(super) struct SpoolFile {
    pub(super) path: PathBuf,
    _owner: Arc<Owner>,
}

impl SpoolFile {
    /// Create a new empty spool file in `owner`'s folder and open it for writing.
    pub(super) fn create(owner: &Arc<Owner>) -> io::Result<(Self, File)> {
        let path = owner.dir.join(format!(
            "{}-{}.spool",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let spool = Self {
            path,
            _owner: Arc::clone(owner),
        };
        Ok((spool, file))
    }
}

impl Drop for SpoolFile {
    /// Delete the file; a Windows open handle is retried by the next sweep.
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %self.path.display(), "compare spool file could not be removed");
        }
    }
}

/// Write `head`, then the rest of `reader` to its end, checking cancel between 1 MiB chunks.
/// Fails with `DiskFull` when the disk or quota is full, `Stopped` on cancel.
pub(super) fn spool_copy(
    reader: &mut dyn Read,
    out: &mut dyn Write,
    head: &[u8],
    cancel: &CancelToken,
) -> Result<(), Reason> {
    out.write_all(head).map_err(write_reason)?;
    let mut buffer = vec![0u8; COPY_CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        match reader.read(&mut buffer) {
            Ok(0) => return out.flush().map_err(write_reason),
            Ok(count) => out.write_all(&buffer[..count]).map_err(write_reason)?,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                tracing::warn!(%error, "compare spool source could not be read");
                return Err(Reason::ReadError);
            }
        }
    }
}

/// The reason for a failed spool write: a full disk is named, anything else is logged.
pub(super) fn write_reason(error: io::Error) -> Reason {
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded | io::ErrorKind::FileTooLarge => {
            Reason::DiskFull
        }
        _ => {
            tracing::warn!(%error, "compare spool could not be written");
            Reason::ReadError
        }
    }
}

/// The shared owner for `base`, so one process holds one folder lock.
pub(super) fn owner_for(base: &Path) -> Option<Arc<Owner>> {
    let owners = OWNERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut owners = owners.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(owner) = owners.get(base).and_then(Weak::upgrade) {
        return Some(owner);
    }
    match Owner::open(base) {
        Ok(owner) => {
            let owner = Arc::new(owner);
            owners.insert(base.to_owned(), Arc::downgrade(&owner));
            Some(owner)
        }
        Err(error) => {
            tracing::warn!(%error, path = %base.display(), "compare spool owner could not open");
            None
        }
    }
}

/// The gate file in `base`, created with the folder when missing.
fn open_gate(base: &Path) -> io::Result<File> {
    fs::create_dir_all(base)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(base.join(GATE))
}

/// Remove every process folder whose owner lock this process can take.
pub(in crate::compare::diff) fn sweep(base: &Path) {
    sweep_with(base, &mut |_| {});
}

/// `sweep`, calling `before_delete` with each folder just before it is removed. Skips the
/// whole sweep while an owner is creating its folder.
pub(in crate::compare::diff) fn sweep_with(base: &Path, before_delete: &mut dyn FnMut(&Path)) {
    match fs::metadata(base) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, path = %base.display(), "compare spool could not be read");
            return;
        }
    }
    let gate = match open_gate(base).map(|gate| gate.try_lock().map(|()| gate)) {
        Ok(Ok(gate)) => gate,
        Ok(Err(error)) => {
            tracing::info!(%error, "compare spool sweep skipped: an owner is starting");
            return;
        }
        Err(error) => {
            tracing::warn!(%error, path = %base.display(), "compare spool gate could not open");
            return;
        }
    };
    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, path = %base.display(), "compare spool could not be read");
            return;
        }
    };
    for entry in entries.flatten() {
        // A link or junction is never followed: only real folders are swept.
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            sweep_folder(&entry.path(), before_delete);
        }
    }
    drop(gate);
}

/// Remove one folder while holding its free lock; a missing lock means a leftover that never
/// finished starting. A lock that is held or cannot be read keeps the folder.
fn sweep_folder(path: &Path, before_delete: &mut dyn FnMut(&Path)) {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.join(OWNER_LOCK));
    let held = match lock {
        Ok(file) => match file.try_lock() {
            Ok(()) => Some(file),
            Err(fs::TryLockError::WouldBlock) => return,
            Err(fs::TryLockError::Error(error)) => {
                tracing::warn!(%error, path = %path.display(), "compare spool lock could not be taken");
                return;
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "compare spool lock could not be read");
            return;
        }
    };
    before_delete(path);
    if let Err(error) = fs::remove_dir_all(path) {
        tracing::warn!(%error, path = %path.display(), "compare spool folder could not be removed");
    }
    drop(held);
}
