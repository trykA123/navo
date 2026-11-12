//! Strict replacement and persistence barriers for synced sibling files.
use std::{fs, io, path::Path};

/// Replaces a target with a synced, create-new sibling on a local filesystem.
/// Caller excludes symlinks/reparse redirection; errors include post-replace flush failures.
pub fn native_replace_durable(source: &Path, target: &Path) -> io::Result<()> {
    let parent = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
    if source.parent() != Some(parent) || source == target {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // On Windows File::sync_all calls FlushFileBuffers on the volume handle.
    replace_with(
        open_barrier(parent),
        || replace(source, target),
        |h| h.sync_all(),
    )
}

/// Acquires a barrier handle, replaces once, then persists; any failure stops work.
fn replace_with<H>(
    handle: io::Result<H>,
    replace: impl FnOnce() -> io::Result<()>,
    flush: impl FnOnce(H) -> io::Result<()>,
) -> io::Result<()> {
    let handle = handle?;
    replace()?;
    flush(handle)
}

#[cfg(not(windows))]
use fs::rename as replace;

/// Opens the Unix parent before replacement so its rename can be persisted.
#[cfg(not(windows))]
fn open_barrier(parent: &Path) -> io::Result<fs::File> {
    fs::File::open(parent.join("."))
}

#[cfg(windows)]
use windows::{open_barrier, replace};

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Windows volume and write-through APIs lack safe wrappers"
)]
mod windows {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::{ffi::OsStrExt, ffi::OsStringExt, fs::OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    /// Opens the actual containing local volume for flushing; lookup/access failures stop replacement.
    pub(super) fn open_barrier(parent: &Path) -> io::Result<fs::File> {
        let name = wide(&fs::canonicalize(parent.join("."))?)?;
        let mut mount = vec![0; name.len() + 1];
        let capacity = u32::try_from(mount.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
        let mut guid = [0; 50];
        // SAFETY: input is absolute and NUL-terminated; output buffers match the supplied capacities.
        unsafe {
            checked(GetVolumePathNameW(
                name.as_ptr(),
                mount.as_mut_ptr(),
                capacity,
            ))?;
            checked(GetVolumeNameForVolumeMountPointW(
                mount.as_ptr(),
                guid.as_mut_ptr(),
                50,
            ))?;
        }
        let mut guid: Vec<u16> = guid.into_iter().take_while(|&unit| unit != 0).collect();
        guid.pop(); // A volume handle must omit the root directory's trailing backslash.
        fs::File::options()
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(OsString::from_wide(&guid))
    }

    /// Replaces once using write-through; access/sharing failures never fall back.
    pub(super) fn replace(source: &Path, target: &Path) -> io::Result<()> {
        let (source, target) = (wide(source)?, wide(target)?);
        // SAFETY: the NUL-terminated paths outlive this synchronous call.
        checked(unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
    }

    /// Encodes a Windows path as UTF-16, rejecting embedded NULs.
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        if path.as_os_str().encode_wide().any(|unit| unit == 0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(path.as_os_str().encode_wide().chain([0]).collect())
    }

    /// Converts a failed Windows API call to its original system error.
    fn checked(ok: i32) -> io::Result<()> {
        (ok != 0).then_some(()).ok_or_else(io::Error::last_os_error)
    }
}

#[cfg(test)]
#[path = "durable_replace_tests.rs"]
mod tests;
