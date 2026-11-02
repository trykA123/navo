#![allow(
    unsafe_code,
    reason = "drive discovery requires read-only Win32 queries"
)]

use std::{io, ptr};

use windows_sys::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE};

use super::config::Drive;
use super::elevation::wide;

pub(super) fn discover() -> io::Result<Vec<Drive>> {
    // SAFETY: GetLogicalDrives has no pointer arguments and queries the local drive mask.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(('A'..='Z')
        .filter(|&letter| mask & (1 << (letter as u32 - 'A' as u32)) != 0)
        .filter_map(inspect)
        .collect())
}

fn inspect(letter: char) -> Option<Drive> {
    let root = wide(&format!("{letter}:\\"));
    // SAFETY: root is a live NUL-terminated drive root.
    let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
    if !matches!(kind, DRIVE_FIXED | DRIVE_REMOVABLE) {
        return None;
    }
    let mut serial = 0;
    let mut filesystem = [0u16; 32];
    // SAFETY: root is valid; serial and filesystem are writable and match their declared lengths.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            &mut serial,
            ptr::null_mut(),
            ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    if ok == 0 {
        return None;
    }
    let length = filesystem
        .iter()
        .position(|&ch| ch == 0)
        .unwrap_or(filesystem.len());
    Some(Drive {
        letter,
        serial: u64::from(serial),
        fixed: kind == DRIVE_FIXED,
        ntfs: String::from_utf16_lossy(&filesystem[..length]).eq_ignore_ascii_case("NTFS"),
    })
}
