//! Windows read-only file handles shared with the sandboxed PDF helper.
//! The parent duplicates an open `File` into the helper; the helper adopts the value as a `File`.

use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, FromRawHandle};
use std::ptr;

use windows_sys::Win32::Foundation::{DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_TYPE_DISK, GetFileType};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Duplicates `file` read-only into `process`; returns the handle value valid inside `process`.
/// `process` needs `PROCESS_DUP_HANDLE`; fails with the OS error, for example when it has exited.
pub fn share_read_handle(file: &File, process: BorrowedHandle<'_>) -> io::Result<u64> {
    let mut value: HANDLE = ptr::null_mut();
    // SAFETY: the current-process pseudo handle needs no close; `file` and `process` are live borrowed handles for the call;
    // `value` is a live, writable HANDLE; the duplicate lives in `process`, which owns and closes it.
    let ok = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            file.as_raw_handle(),
            process.as_raw_handle(),
            &raw mut value,
            FILE_GENERIC_READ,
            0,
            0,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value.expose_provenance() as u64)
}

/// Adopts a handle value the parent shared into this process as an owned `File`.
/// Fails with `InvalidInput` for 0, -1 or a non-disk handle; a refused value is not closed.
///
/// # Safety
///
/// The caller passes only the value from the parent's latest duplication into this helper
/// process, and adopts it once per duplication. Windows may reuse a value after its handle
/// closes, so a repeated value from a new duplication is valid. Ownership is then exclusive:
/// the parent closes nothing it duplicated into the helper.
pub unsafe fn adopt_shared_file(value: u64) -> io::Result<File> {
    let Ok(bits) = usize::try_from(value) else {
        return Err(refused("the handle value does not fit this process"));
    };
    let handle: HANDLE = ptr::with_exposed_provenance_mut(bits);
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(refused("the handle value is empty"));
    }
    // SAFETY: GetFileType only reads the handle value; an unknown value answers FILE_TYPE_UNKNOWN.
    let kind = unsafe { GetFileType(handle) };
    if kind != FILE_TYPE_DISK {
        return Err(refused("the shared handle is not a disk file"));
    }
    // SAFETY: the caller's contract makes this process the only owner of `handle`, adopted once.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// Builds the `InvalidInput` error for a refused handle value.
fn refused(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}
