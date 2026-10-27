#![allow(
    unsafe_code,
    reason = "std has no safe wrapper for querying a volume's storage properties"
)]

use std::{mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, OPEN_EXISTING,
    },
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            DEVICE_SEEK_PENALTY_DESCRIPTOR, DEVICE_TRIM_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY,
            PropertyStandardQuery, STORAGE_PROPERTY_QUERY, StorageDeviceSeekPenaltyProperty,
            StorageDeviceTrimProperty,
        },
        WindowsProgramming::{DRIVE_NO_ROOT_DIR, DRIVE_REMOTE, DRIVE_REMOVABLE, DRIVE_UNKNOWN},
    },
};

use super::{DriveClass, classify};

struct VolumeHandle(HANDLE);

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: this handle came from CreateFileW and is closed only here.
        unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn probe(volume: &str) -> DriveClass {
    if volume.starts_with(r"\\") {
        return classify(true, None, None);
    }
    if !matches!(volume.as_bytes(), [b'A'..=b'Z', b':']) {
        return DriveClass::Unknown;
    }
    let root: Vec<u16> = format!("{volume}\\").encode_utf16().chain([0]).collect();
    // SAFETY: root is a NUL-terminated drive root.
    match unsafe { GetDriveTypeW(root.as_ptr()) } {
        DRIVE_REMOTE => classify(true, None, None),
        DRIVE_REMOVABLE => DriveClass::Removable,
        DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR => DriveClass::Unknown,
        _ => storage_class(volume),
    }
}

fn storage_class(volume: &str) -> DriveClass {
    let Ok(handle) = open_volume(volume) else {
        return DriveClass::Unknown;
    };
    let seek_penalty = query_seek_penalty(&handle);
    let trim_enabled = if seek_penalty.is_none() {
        query_trim(&handle)
    } else {
        None
    };
    classify(false, seek_penalty, trim_enabled)
}

fn open_volume(volume: &str) -> Result<VolumeHandle, u32> {
    let path: Vec<u16> = format!(r"\\.\{volume}").encode_utf16().chain([0]).collect();
    // SAFETY: path is NUL-terminated; security and template pointers are null.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        // SAFETY: GetLastError reads the calling thread's error from CreateFileW.
        return Err(unsafe { GetLastError() });
    }
    Ok(VolumeHandle(handle))
}

fn query_seek_penalty(handle: &VolumeHandle) -> Option<bool> {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        ..STORAGE_PROPERTY_QUERY::default()
    };
    let mut descriptor = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
    let query_size = u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>()).ok()?;
    let descriptor_size = u32::try_from(size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>()).ok()?;
    let mut returned = 0;
    // SAFETY: handle is open; the typed buffers have the supplied sizes and outlive the call.
    let success = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const query).cast(),
            query_size,
            (&raw mut descriptor).cast(),
            descriptor_size,
            &raw mut returned,
            ptr::null_mut(),
        )
    };
    (success != 0 && returned >= descriptor_size && descriptor.Size >= descriptor_size)
        .then_some(descriptor.IncursSeekPenalty)
}

fn query_trim(handle: &VolumeHandle) -> Option<bool> {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceTrimProperty,
        QueryType: PropertyStandardQuery,
        ..STORAGE_PROPERTY_QUERY::default()
    };
    let mut descriptor = DEVICE_TRIM_DESCRIPTOR::default();
    let query_size = u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>()).ok()?;
    let descriptor_size = u32::try_from(size_of::<DEVICE_TRIM_DESCRIPTOR>()).ok()?;
    let mut returned = 0;
    // SAFETY: handle is open; the typed buffers have the supplied sizes and outlive the call.
    let success = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const query).cast(),
            query_size,
            (&raw mut descriptor).cast(),
            descriptor_size,
            &raw mut returned,
            ptr::null_mut(),
        )
    };
    (success != 0 && returned >= descriptor_size && descriptor.Size >= descriptor_size)
        .then_some(descriptor.TrimEnabled)
}

#[cfg(test)]
pub(super) fn query_errors(volume: &str) -> (u32, u32) {
    use windows_sys::Win32::Foundation::SetLastError;

    let handle = match open_volume(volume) {
        Ok(handle) => handle,
        Err(error) => return (error, error),
    };
    // SAFETY: SetLastError clears only the calling thread's error state.
    unsafe { SetLastError(0) };
    let _ = query_seek_penalty(&handle);
    // SAFETY: GetLastError reads the calling thread's error before another Win32 call.
    let seek_error = unsafe { GetLastError() };
    // SAFETY: SetLastError clears only the calling thread's error state.
    unsafe { SetLastError(0) };
    let _ = query_trim(&handle);
    // SAFETY: GetLastError reads the calling thread's error before another Win32 call.
    let trim_error = unsafe { GetLastError() };
    (seek_error, trim_error)
}
