#![allow(
    unsafe_code,
    reason = "std exposes the volume serial number only as an unstable API"
)]

use std::fs::OpenOptions;
use std::io;
use std::mem::MaybeUninit;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
};

pub(super) fn volume_serial(folder: &Path) -> io::Result<u64> {
    let handle = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(folder)?;
    let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    // SAFETY: the handle is open for the whole call and info is a writable, zeroed struct.
    let ok = unsafe { GetFileInformationByHandle(handle.as_raw_handle(), info.as_mut_ptr()) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call succeeded, so the system filled the struct.
    Ok(u64::from(
        unsafe { info.assume_init() }.dwVolumeSerialNumber,
    ))
}
