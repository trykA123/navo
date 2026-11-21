//! The durable disk layer: every file step the write journal takes, behind one trait.
//! `RealDisk` is the OS; a fault layer (D2) wraps it to fail or crash at a chosen step.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::Path;

use crate::FileIdentity;

#[cfg(feature = "test-support")]
mod fault;
#[cfg(test)]
mod fault_crash_tests;
#[cfg(test)]
mod fault_tests;
mod real;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod windows_reparse;
#[cfg(test)]
mod windows_tests;

#[cfg(feature = "test-support")]
pub use fault::{FaultDisk, Plan, Tail};
pub use real::RealDisk;

/// What the journal needs to know about one path, read without following a symlink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta {
    /// Length in bytes.
    pub size: u64,
    /// Last write time in ms since the Unix epoch; `None` when the OS cannot say.
    pub modified_ms: Option<i64>,
    /// True when the file has no write permission at all.
    pub read_only: bool,
    /// Volume and file id, used to notice a target that was swapped between check and use.
    pub identity: Option<FileIdentity>,
    /// True only when the volume says it keeps no stable file ids (FAT, exFAT): `identity` is
    /// then `None`, and the journal checks the target by hash instead (NC202).
    pub no_stable_ids: bool,
}

/// Why a `Disk::replace` failed, and where the old target is now.
#[derive(Debug)]
pub enum ReplaceError {
    /// The target was not swapped: it still holds its old bytes.
    Unchanged(io::Error),
    /// The target was swapped, but the swap is not durable; roll back by hash (NC145).
    NotDurable(io::Error),
    /// The swap failed and the target name may be gone; the old file may be at `old`
    /// (Windows 1176/1177, or the target found missing). Roll back by hash (NC145).
    TargetMoved(io::Error),
}

impl fmt::Display for ReplaceError {
    /// Names the case and the OS error behind it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unchanged(error) => write!(f, "replace failed, target unchanged: {error}"),
            Self::NotDurable(error) => write!(f, "replace done but not durable: {error}"),
            Self::TargetMoved(error) => write!(f, "replace failed, target moved aside: {error}"),
        }
    }
}

impl std::error::Error for ReplaceError {
    /// The OS error behind the failure.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unchanged(error) | Self::NotDurable(error) | Self::TargetMoved(error) => {
                Some(error)
            }
        }
    }
}

/// A file being written; `sync` makes its bytes and metadata durable.
pub trait DiskFile: Write + Send {
    /// Flushes data and metadata to the device; fails when the device reports an error.
    fn sync(&mut self) -> io::Result<()>;
}

/// The file steps of the write journal. Each call is one numbered op for the fault layer.
/// No call syncs a folder unless it says so: callers sync with `sync_dir`.
pub trait Disk: Send + Sync {
    /// Creates a new file for writing; fails with `AlreadyExists` if the name exists.
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>>;
    /// Opens an existing file to append to it; never creates it.
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>>;
    /// Opens an existing file for reading.
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>>;
    /// Reads the metadata of a file or folder; `None` when the path is missing.
    /// Fails with `InvalidInput` on a symlink or special file, so no write ever goes through one.
    fn meta(&self, path: &Path) -> io::Result<Option<Meta>>;
    /// Sets the last write time, in ms since the Unix epoch.
    fn set_modified(&self, path: &Path, ms: i64) -> io::Result<()>;
    /// Copies the permission bits of `from` to `to`; a no-op on Windows.
    fn copy_mode(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Atomically puts `staged` in place of `target` and makes it durable; `old` is a temp name
    /// on Windows. A failed removal of `old` after the swap leaves a stray temp, which the
    /// job's rollback deletes (NC145 step 1); nothing sweeps it after a `Commit` yet.
    fn replace(&self, staged: &Path, target: &Path, old: &Path) -> Result<(), ReplaceError>;
    /// Renames `from` to `to`; fails with `AlreadyExists` and changes nothing if `to` exists.
    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes one file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Creates one folder; its parent must exist.
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    /// Removes a folder and everything in it, without following symlinks.
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Lists the entry names of a folder, sorted; fails with `InvalidData` on a non-UTF-8 name.
    fn list(&self, dir: &Path) -> io::Result<Vec<String>>;
    /// Makes the folder's entries (creates, renames, removes) durable; `Ok` on Windows.
    fn sync_dir(&self, dir: &Path) -> io::Result<()>;
}
