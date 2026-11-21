//! `RealDisk`: the `Disk` trait on the OS. Linux is built here (rustix is a Linux-only
//! dependency), Windows in `windows.rs`; elsewhere every call returns `Unsupported`.

use std::fs::File;
use std::io;

#[cfg(target_os = "linux")]
use std::{fs::OpenOptions, io::Read};
#[cfg(any(target_os = "linux", windows))]
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "linux")]
use super::{Disk, DiskFile, Meta, ReplaceError};

/// The OS file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealDisk;

impl super::DiskFile for File {
    /// `fsync`: data and metadata (size, mode, times), not only data.
    fn sync(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}

/// Converts a file time to ms since the Unix epoch; `None` past the `i64` range.
#[cfg(any(target_os = "linux", windows))]
pub(super) fn to_ms(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis())
            .ok()
            .map(|ms| -ms),
    }
}

/// Converts ms since the Unix epoch to a file time; fails with `InvalidInput` out of range.
#[cfg(any(target_os = "linux", windows))]
pub(super) fn from_ms(ms: i64) -> io::Result<SystemTime> {
    let span = Duration::from_millis(ms.unsigned_abs());
    let time = if ms >= 0 {
        UNIX_EPOCH.checked_add(span)
    } else {
        UNIX_EPOCH.checked_sub(span)
    };
    time.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "time out of range"))
}

/// The folder that holds `path`; fails with `InvalidInput` for a root or a bare name.
#[cfg(any(target_os = "linux", windows))]
pub(super) fn parent(path: &Path) -> io::Result<&Path> {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent folder"))
}

/// Maps a rename result and the folder sync after it to a replace result.
#[cfg(target_os = "linux")]
pub(super) fn swap(
    renamed: io::Result<()>,
    sync: impl FnOnce() -> io::Result<()>,
) -> Result<(), ReplaceError> {
    renamed.map_err(ReplaceError::Unchanged)?;
    sync().map_err(ReplaceError::NotDurable)
}

/// Maps a failed `ReplaceFileW` to a replace result (NC146). `moved_aside` says whether the
/// target name is gone while `old` exists; that, a failed probe, 1176 or 1177 give
/// `TargetMoved`, so the caller rolls back by hash. Any other failure is `Unchanged`.
#[cfg(any(windows, test))]
pub(super) fn replace_failure(
    error: io::Error,
    moved_aside: io::Result<bool>,
) -> super::ReplaceError {
    match (error.raw_os_error(), moved_aside) {
        (Some(1176 | 1177), _) | (_, Ok(true) | Err(_)) => super::ReplaceError::TargetMoved(error),
        _ => super::ReplaceError::Unchanged(error),
    }
}

/// An `InvalidInput` error that names why a path was refused.
#[cfg(any(target_os = "linux", windows))]
pub(super) fn refused(why: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, why)
}

/// Opens a regular file to read without following a symlink or blocking on a FIFO.
/// Fails with `InvalidInput` on a symlink or anything that is not a regular file.
#[cfg(target_os = "linux")]
fn open_regular(path: &Path) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags};
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let file = match rustix::fs::open(path, flags, Mode::empty()) {
        Err(rustix::io::Errno::LOOP) => return Err(refused("path is a symlink")),
        opened => File::from(opened?),
    };
    if !file.metadata()?.is_file() {
        return Err(refused("not a regular file"));
    }
    Ok(file)
}

/// True when this process may not write `path`: no access for our user, or a read-only mount.
#[cfg(target_os = "linux")]
fn write_denied(path: &Path) -> io::Result<bool> {
    use rustix::{fs::Access, io::Errno};
    match rustix::fs::access(path, Access::WRITE_OK) {
        Ok(()) => Ok(false),
        Err(Errno::ACCESS | Errno::PERM | Errno::ROFS) => Ok(true),
        Err(errno) => Err(errno.into()),
    }
}

/// The entry names of a folder, sorted; fails with `InvalidData` on a non-UTF-8 name.
#[cfg(any(target_os = "linux", windows))]
pub(super) fn list_sorted(dir: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().into_string();
        names.push(name.map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 name"))?);
    }
    names.sort_unstable();
    Ok(names)
}

#[cfg(target_os = "linux")]
impl Disk for RealDisk {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        Ok(Box::new(
            OpenOptions::new().write(true).create_new(true).open(path)?,
        ))
    }

    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        Ok(Box::new(OpenOptions::new().append(true).open(path)?))
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(open_regular(path)?))
    }

    fn meta(&self, path: &Path) -> io::Result<Option<Meta>> {
        let found = match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            found => found?,
        };
        if !(found.is_file() || found.is_dir()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a file or folder",
            ));
        }
        Ok(Some(Meta {
            size: found.len(),
            modified_ms: found.modified().ok().and_then(to_ms),
            read_only: write_denied(path)?,
            identity: Some(crate::native::identity::from_metadata(&found)),
            no_stable_ids: false,
        }))
    }

    fn set_modified(&self, path: &Path, ms: i64) -> io::Result<()> {
        open_regular(path)?.set_modified(from_ms(ms)?)
    }

    fn copy_mode(&self, from: &Path, to: &Path) -> io::Result<()> {
        let source = std::fs::symlink_metadata(from)?;
        if !source.is_file() {
            return Err(refused("mode source is not a regular file"));
        }
        open_regular(to)?.set_permissions(source.permissions())
    }

    /// `rename`, then sync the folder; see `swap` for how failures map. Both files must
    /// share a folder, so the rename stays on one volume and one folder sync covers it.
    fn replace(&self, staged: &Path, target: &Path, _old: &Path) -> Result<(), ReplaceError> {
        let dir = parent(target).map_err(ReplaceError::Unchanged)?;
        if parent(staged).ok() != Some(dir) {
            return Err(ReplaceError::Unchanged(refused(
                "staged file is in another folder",
            )));
        }
        swap(std::fs::rename(staged, target), || self.sync_dir(dir))
    }

    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE).map_err(|errno| {
            if errno == rustix::io::Errno::EXIST {
                io::ErrorKind::AlreadyExists.into()
            } else {
                io::Error::from(errno)
            }
        })
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
        list_sorted(dir)
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }
}

/// Off Linux and Windows, every call fails closed with `Unsupported`.
#[cfg(not(any(target_os = "linux", windows)))]
impl super::Disk for RealDisk {
    fn create_new(&self, _: &std::path::Path) -> io::Result<Box<dyn super::DiskFile>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn open_append(&self, _: &std::path::Path) -> io::Result<Box<dyn super::DiskFile>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn open_read(&self, _: &std::path::Path) -> io::Result<Box<dyn io::Read + Send>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn meta(&self, _: &std::path::Path) -> io::Result<Option<super::Meta>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn set_modified(&self, _: &std::path::Path, _: i64) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn copy_mode(&self, _: &std::path::Path, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn replace(
        &self,
        _: &std::path::Path,
        _: &std::path::Path,
        _: &std::path::Path,
    ) -> Result<(), super::ReplaceError> {
        Err(super::ReplaceError::Unchanged(
            io::ErrorKind::Unsupported.into(),
        ))
    }
    fn rename_new(&self, _: &std::path::Path, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn remove_file(&self, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn create_dir(&self, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn remove_dir_all(&self, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn list(&self, _: &std::path::Path) -> io::Result<Vec<String>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn sync_dir(&self, _: &std::path::Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
