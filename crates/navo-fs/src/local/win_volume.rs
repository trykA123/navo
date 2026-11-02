#![allow(
    unsafe_code,
    reason = "std exposes the volume serial number only as an unstable API"
)]

use std::fs::OpenOptions;
use std::io;
use std::mem::MaybeUninit;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
    GetVolumePathNameW,
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

pub(super) fn mount_point(path: &Path) -> io::Result<Vec<u16>> {
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0u16; name.len() + 2];
    let capacity = u32::try_from(buffer.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: name is NUL-terminated and buffer holds capacity writable UTF-16 units.
    let ok = unsafe { GetVolumePathNameW(name.as_ptr(), buffer.as_mut_ptr(), capacity) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    buffer.truncate(end);
    Ok(comparable(&buffer))
}

pub(super) fn is_mount_point(path: &Path, mount: &[u16]) -> bool {
    comparable(&path.as_os_str().encode_wide().collect::<Vec<_>>()) == mount
}

fn comparable(units: &[u16]) -> Vec<u16> {
    let text = String::from_utf16_lossy(units);
    let plain = if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    };
    plain
        .trim_end_matches('\\')
        .to_ascii_lowercase()
        .encode_utf16()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verbatim_drive_matches_its_plain_mount_point() {
        let mount = comparable(&"C:\\".encode_utf16().collect::<Vec<_>>());
        assert!(is_mount_point(Path::new(r"\\?\c:\"), &mount));
        assert!(!is_mount_point(Path::new(r"\\?\C:\mnt"), &mount));
        let share = comparable(&r"\\?\UNC\host\share\".encode_utf16().collect::<Vec<_>>());
        assert!(is_mount_point(Path::new(r"\\host\share"), &share));
    }

    #[test]
    fn the_system_drive_root_is_the_mount_point_of_a_temp_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mount = mount_point(dir.path()).unwrap();
        assert!(
            dir.path()
                .ancestors()
                .any(|level| is_mount_point(level, &mount))
        );
    }
}
