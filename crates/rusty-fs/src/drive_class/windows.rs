#![allow(
    unsafe_code,
    reason = "std has no safe wrapper for querying a volume's seek penalty"
)]

use std::{mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, OPEN_EXISTING,
    },
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
            STORAGE_PROPERTY_QUERY, StorageDeviceSeekPenaltyProperty,
        },
        WindowsProgramming::{DRIVE_NO_ROOT_DIR, DRIVE_REMOTE, DRIVE_UNKNOWN},
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
        return classify(true, None);
    }
    if !matches!(volume.as_bytes(), [b'A'..=b'Z', b':']) {
        return DriveClass::Unknown;
    }
    let root: Vec<u16> = format!("{volume}\\").encode_utf16().chain([0]).collect();
    // SAFETY: root is a NUL-terminated drive root.
    match unsafe { GetDriveTypeW(root.as_ptr()) } {
        DRIVE_REMOTE => classify(true, None),
        DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR => DriveClass::Unknown,
        _ => classify(false, seek_penalty(volume)),
    }
}

fn seek_penalty(volume: &str) -> Option<bool> {
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
        return None;
    }
    query_seek_penalty(&VolumeHandle(handle))
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
