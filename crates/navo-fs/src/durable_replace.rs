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

/// Replaces a target with a synced, create-new sibling for non-elevated user-space callers.
/// Caller excludes symlinks/reparse redirection. On Windows, falls back to direct replacement
/// without a raw-volume flush when opening the volume handle fails with `PermissionDenied`.
pub fn native_replace_user_durable(source: &Path, target: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        native_replace_durable(source, target)
    }
    #[cfg(windows)]
    {
        let parent = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
        if source.parent() != Some(parent) || source == target {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        // Raw volume write handles require Administrator privileges on Windows; skip only the
        // volume flush barrier when open_barrier returns PermissionDenied so normal users still
        // get verbatim-path POSIX/write-through replacement with full target ACL enforcement.
        replace_user_with(
            open_barrier(parent),
            || replace(source, target),
            |h| h.sync_all(),
        )
    }
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

/// Acquires an optional barrier handle, replaces once, and flushes only when a handle was
/// acquired. Tolerates `PermissionDenied` from barrier acquisition while propagating all
/// other barrier faults and every replace or flush error.
#[cfg(any(windows, test))]
fn replace_user_with<H>(
    barrier: io::Result<H>,
    replace: impl FnOnce() -> io::Result<()>,
    flush: impl FnOnce(H) -> io::Result<()>,
) -> io::Result<()> {
    match barrier {
        Ok(handle) => {
            replace()?;
            flush(handle)
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => replace(),
        Err(error) => Err(error),
    }
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
    reason = "Windows volume, POSIX rename, and write-through APIs lack safe wrappers"
)]
mod windows {
    use super::*;
    use std::ffi::OsString;
    use std::mem::offset_of;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileRenameInfoEx, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, SYNCHRONIZE,
        SetFileInformationByHandle,
    };

    /// `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` — overwrite an existing target name.
    const RENAME_REPLACE_IF_EXISTS: u32 = 0x0000_0001;
    /// `FILE_RENAME_FLAG_POSIX_SEMANTICS` — atomically unlink an open target shared with DELETE.
    const RENAME_POSIX_SEMANTICS: u32 = 0x0000_0002;

    /// Opens the actual containing local volume for flushing; lookup/access failures stop replacement.
    pub(super) fn open_barrier(parent: &Path) -> io::Result<fs::File> {
        let name = wide_nul(&fs::canonicalize(parent.join("."))?)?;
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

    /// Replaces `target` with `source` using verbatim (`\\?\`) paths so long names (>260 chars)
    /// succeed even with `LongPathsEnabled=0`. Uses NTFS POSIX rename (`FileRenameInfoEx`) so
    /// targets with open `FILE_SHARE_DELETE` readers (such as `PagedFile`) replace atomically;
    /// falls back to `MoveFileExW` only when the filesystem does not support `FileRenameInfoEx`.
    pub(super) fn replace(source: &Path, target: &Path) -> io::Result<()> {
        let source_units = verbatim_units(source)?;
        let target_units = verbatim_units(target)?;
        match posix_rename(&source_units, &target_units) {
            Ok(()) => Ok(()),
            Err(error) if is_posix_rename_unsupported(&error) => {
                move_file_ex(&source_units, &target_units)
            }
            Err(error) => Err(error),
        }
    }

    fn is_posix_rename_unsupported(error: &io::Error) -> bool {
        let Some(code) = error.raw_os_error().and_then(|raw| u32::try_from(raw).ok()) else {
            return false;
        };
        matches!(
            code,
            ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER
        )
    }

    fn posix_rename(source_units: &[u16], target_units: &[u16]) -> io::Result<()> {
        let source_path = OsString::from_wide(source_units);
        let handle = fs::OpenOptions::new()
            .access_mode(DELETE | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(source_path)?;

        let name_bytes = target_units
            .len()
            .checked_mul(size_of::<u16>())
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or(io::ErrorKind::InvalidInput)?;
        let header_size = offset_of!(FILE_RENAME_INFO, FileName);
        let total_bytes = header_size
            .checked_add(target_units.len().saturating_add(1) * size_of::<u16>())
            .ok_or(io::ErrorKind::InvalidInput)?;
        let total_len = u32::try_from(total_bytes).map_err(|_| io::ErrorKind::InvalidInput)?;
        let mut storage = vec![0u64; total_bytes.div_ceil(size_of::<u64>())];
        let base = storage.as_mut_ptr().cast::<u8>();

        // SAFETY: `storage` is 8-byte aligned (matching `FILE_RENAME_INFO`), zeroed, and sized to
        // hold the header plus `target_units` and a trailing NUL code unit.
        unsafe {
            let info = base.cast::<FILE_RENAME_INFO>();
            (*info).Anonymous.Flags = RENAME_REPLACE_IF_EXISTS | RENAME_POSIX_SEMANTICS;
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = name_bytes;
            let dest = base.add(header_size).cast::<u16>();
            std::ptr::copy_nonoverlapping(target_units.as_ptr(), dest, target_units.len());
            checked(SetFileInformationByHandle(
                handle.as_raw_handle(),
                FileRenameInfoEx,
                base.cast(),
                total_len,
            ))
        }
    }

    fn move_file_ex(source_units: &[u16], target_units: &[u16]) -> io::Result<()> {
        let mut src = Vec::with_capacity(source_units.len() + 1);
        src.extend_from_slice(source_units);
        src.push(0);
        let mut dst = Vec::with_capacity(target_units.len() + 1);
        dst.extend_from_slice(target_units);
        dst.push(0);
        // SAFETY: both buffers are NUL-terminated and live through the synchronous Win32 call.
        checked(unsafe {
            MoveFileExW(
                src.as_ptr(),
                dst.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
    }

    /// Encodes `path` as a verbatim UTF-16 sequence (`\\?\...` or `\\?\UNC\...`) without trailing NUL.
    fn verbatim_units(path: &Path) -> io::Result<Vec<u16>> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::path::absolute(path)?
        };
        let units: Vec<u16> = absolute.as_os_str().encode_wide().collect();
        if units.contains(&0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(to_verbatim_utf16(&units))
    }

    /// Encodes a Windows path as NUL-terminated UTF-16, rejecting embedded NULs.
    fn wide_nul(path: &Path) -> io::Result<Vec<u16>> {
        let mut units = verbatim_units(path)?;
        units.push(0);
        Ok(units)
    }

    fn to_verbatim_utf16(units: &[u16]) -> Vec<u16> {
        const BACKSLASH: u16 = b'\\' as u16;
        const SLASH: u16 = b'/' as u16;
        const QUESTION: u16 = b'?' as u16;
        const DOT: u16 = b'.' as u16;
        const COLON: u16 = b':' as u16;
        let is_sep = |u: u16| u == BACKSLASH || u == SLASH;

        match units {
            [a, b, mark, c, ..]
                if is_sep(*a)
                    && is_sep(*b)
                    && (*mark == QUESTION || *mark == DOT)
                    && is_sep(*c) =>
            {
                units.to_vec()
            }
            [a, b, rest @ ..] if is_sep(*a) && is_sep(*b) => {
                let mut out: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
                append_normalized(&mut out, rest, 2);
                out
            }
            [drive, COLON, sep, rest @ ..]
                if u8::try_from(*drive).is_ok_and(|b| b.is_ascii_alphabetic()) && is_sep(*sep) =>
            {
                let mut out: Vec<u16> = r"\\?\".encode_utf16().collect();
                out.extend([*drive, COLON, BACKSLASH]);
                append_normalized(&mut out, rest, 0);
                out
            }
            _ => units.to_vec(),
        }
    }

    fn append_normalized(out: &mut Vec<u16>, rest: &[u16], fixed: usize) {
        const BACKSLASH: u16 = b'\\' as u16;
        const SLASH: u16 = b'/' as u16;
        const DOT: u16 = b'.' as u16;
        let mut segments: Vec<&[u16]> = Vec::new();
        for segment in rest.split(|&u| u == BACKSLASH || u == SLASH) {
            match segment {
                [DOT, DOT] if segments.len() > fixed => {
                    segments.pop();
                }
                [] | [DOT] | [DOT, DOT] => {}
                _ => segments.push(segment),
            }
        }
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 {
                out.push(BACKSLASH);
            }
            out.extend_from_slice(segment);
        }
    }

    /// Converts a failed Windows API call to its original system error.
    fn checked(ok: i32) -> io::Result<()> {
        (ok != 0).then_some(()).ok_or_else(io::Error::last_os_error)
    }
}

#[cfg(test)]
#[path = "durable_replace_tests.rs"]
mod tests;
