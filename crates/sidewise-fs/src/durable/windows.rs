//! `RealDisk` on Windows (NC146): every path goes verbatim (`\\?\`), `replace` is
//! `ReplaceFileW` with an `-old` backup name, and `rename_new` never replaces.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_READONLY, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES,
};

use super::real::{from_ms, list_sorted, parent, refused, replace_failure, to_ms};
use super::windows_reparse::{check_plain, on_a_volume};
use super::{Disk, DiskFile, Meta, RealDisk, ReplaceError};

/// The verbatim form of an absolute path, so long names and trailing dots pass unchanged.
/// Fails with `InvalidInput` on a relative path, one with a NUL unit, or a device path.
fn verbatim(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(refused("path is not absolute"));
    }
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(refused("path holds a NUL"));
    }
    let wide = crate::native::verbatim::verbatim(&units);
    if !on_a_volume(&wide) {
        return Err(refused("device path, not a file on a volume"));
    }
    Ok(PathBuf::from(OsString::from_wide(&wide)))
}

/// The verbatim path as a NUL-terminated UTF-16 string for a Win32 call.
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut units: Vec<u16> = verbatim(path)?.as_os_str().encode_wide().collect();
    units.push(0);
    Ok(units)
}

/// Opens a regular file without following a link (symlink or junction).
/// Fails with `InvalidInput` on a link, placeholder, offline file or anything not a file.
fn open_regular(path: &Path, options: &mut OpenOptions) -> io::Result<File> {
    let file = options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(verbatim(path)?)?;
    check_plain(&file)?;
    if !file.metadata()?.is_file() {
        return Err(refused("not a regular file"));
    }
    Ok(file)
}

/// `ReplaceFileW(target, staged, old)`: the target keeps its creation time, attributes and
/// streams. Owner and ACL are merged best effort: without `WRITE_DAC` they drop silently.
/// Fails with the OS error; `replace_failure` maps it.
#[allow(unsafe_code, reason = "ReplaceFileW is a Win32 call")]
fn replace_file(target: &[u16], staged: &[u16], old: &[u16]) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW};
    // SAFETY: the three buffers are NUL-terminated and live through the call; the
    // exclude and reserved pointers must be null.
    let ok = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            staged.as_ptr(),
            old.as_ptr(),
            REPLACEFILE_IGNORE_MERGE_ERRORS,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `MoveFileExW` with write-through and without `REPLACE_EXISTING`: fails with
/// `AlreadyExists` (error 80 or 183) when `to` exists.
#[allow(unsafe_code, reason = "MoveFileExW is a Win32 call")]
fn move_new(from: &[u16], to: &[u16]) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
    // SAFETY: both buffers are NUL-terminated and live through the call.
    let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Flushes the swapped target through a write handle, so the replace is durable (NTFS
/// flushes its log with the file). Fails with the OS error, also when the file is in use.
fn flush(target: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(verbatim(target)?)?
        .sync_all()
}

/// After a failed swap: true when the target name is gone and `old` exists (NC145).
fn moved_aside(target: &Path, old: &Path) -> io::Result<bool> {
    Ok(RealDisk.meta(target)?.is_none() && RealDisk.meta(old)?.is_some())
}

impl Disk for RealDisk {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(verbatim(path)?)?;
        Ok(Box::new(file))
    }

    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        Ok(Box::new(
            OpenOptions::new().append(true).open(verbatim(path)?)?,
        ))
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(open_regular(path, OpenOptions::new().read(true))?))
    }

    /// Reads one `BACKUP_SEMANTICS | OPEN_REPARSE_POINT` handle, never a link's target. A link,
    /// placeholder or offline file is refused. On FAT and exFAT `identity` is `None` and
    /// `no_stable_ids` is true.
    fn meta(&self, path: &Path) -> io::Result<Option<Meta>> {
        let opened = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(verbatim(path)?);
        let file = match opened {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            opened => opened?,
        };
        let found = file.metadata()?;
        if found.is_file() {
            check_plain(&file)?;
        } else if !found.is_dir() {
            return Err(refused("not a file or folder"));
        }
        let (identity, no_stable_ids) = crate::native::identity::for_meta(&file)?;
        Ok(Some(Meta {
            size: found.len(),
            modified_ms: found.modified().ok().and_then(to_ms),
            read_only: found.file_attributes() & FILE_ATTRIBUTE_READONLY != 0,
            identity,
            no_stable_ids,
        }))
    }

    fn set_modified(&self, path: &Path, ms: i64) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES);
        open_regular(path, &mut options)?.set_modified(from_ms(ms)?)
    }

    fn copy_mode(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Both files must share a folder, so the swap stays on one volume. After the swap the
    /// target is flushed, then `old` is removed; a failed removal is logged and leaves a stray.
    fn replace(&self, staged: &Path, target: &Path, old: &Path) -> Result<(), ReplaceError> {
        let dir = parent(target).map_err(ReplaceError::Unchanged)?;
        if parent(staged).ok() != Some(dir) {
            return Err(ReplaceError::Unchanged(refused(
                "staged file is in another folder",
            )));
        }
        let names = || io::Result::Ok((wide(target)?, wide(staged)?, wide(old)?));
        let (target_w, staged_w, old_w) = names().map_err(ReplaceError::Unchanged)?;
        if let Err(error) = replace_file(&target_w, &staged_w, &old_w) {
            let moved = moved_aside(target, old);
            if let Err(probe) = &moved {
                tracing::warn!(path = %target.display(), %probe, "replace: could not probe the target");
            }
            return Err(replace_failure(error, moved));
        }
        let flushed = flush(target);
        if let Err(error) = verbatim(old).and_then(std::fs::remove_file) {
            tracing::warn!(path = %old.display(), %error, "replace: could not remove the old file");
        }
        flushed.map_err(ReplaceError::NotDurable)
    }

    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        move_new(&wide(from)?, &wide(to)?)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(verbatim(path)?)
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(verbatim(path)?)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(verbatim(path)?)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
        list_sorted(&verbatim(dir)?)
    }

    /// A no-op on Windows (NC146).
    fn sync_dir(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }
}
