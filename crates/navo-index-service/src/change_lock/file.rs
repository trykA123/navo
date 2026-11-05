#![allow(unsafe_code, reason = "exclusive change files require Win32 FFI")]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_ALWAYS};
use windows_sys::Win32::UI::Shell::FOLDERID_ProgramFiles;

use crate::directory::{ProtectedDirectory, known_folder};

pub(crate) struct FileLock {
    _handle: OwnedHandle,
    _directory: ProtectedDirectory,
}

impl FileLock {
    pub(super) fn acquire(wait: Duration) -> io::Result<Option<Self>> {
        let started = Instant::now();
        let directory = known_folder(&FOLDERID_ProgramFiles)?.join("Navo");
        let guard = ProtectedDirectory::create_for_lock(&directory)?;
        let handle = super::acquire_file(
            wait.saturating_sub(started.elapsed()),
            || open(&directory.join("change.lock")),
            {
                let retry_started = Instant::now();
                move || retry_started.elapsed()
            },
            std::thread::sleep,
        )?;
        Ok(handle.map(|handle| Self {
            _handle: handle,
            _directory: guard,
        }))
    }
}

fn open(path: &Path) -> io::Result<OwnedHandle> {
    let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: path is terminated and pinned by the directory guard; share mode zero is exclusive.
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW transferred a valid uniquely owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
