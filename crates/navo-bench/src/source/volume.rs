//! Drive kind and file system class of a volume; names and labels are never kept.

use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsClass {
    Ntfs,
    Refs,
    Fat,
    ExFat,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Volume {
    pub kind: DriveKind,
    pub fs: FsClass,
}

impl Volume {
    pub const UNKNOWN: Self = Self {
        kind: DriveKind::Unknown,
        fs: FsClass::Unknown,
    };
}

/// Maps a Win32 `GetDriveType` code.
#[cfg(any(windows, test))]
pub(crate) fn classify_drive(code: u32) -> DriveKind {
    match code {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Network,
        0 | 1 => DriveKind::Unknown,
        _ => DriveKind::Other,
    }
}

#[cfg(any(windows, test))]
pub(crate) fn classify_fs(name: &str) -> FsClass {
    match name.to_ascii_uppercase().as_str() {
        "NTFS" => FsClass::Ntfs,
        "REFS" => FsClass::Refs,
        "FAT" | "FAT32" => FsClass::Fat,
        "EXFAT" => FsClass::ExFat,
        "" => FsClass::Unknown,
        _ => FsClass::Other,
    }
}

#[cfg(not(windows))]
pub(super) fn probe(_root: &Path) -> Volume {
    Volume::UNKNOWN
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "volume queries pass owned, correctly sized UTF-16 buffers"
)]
pub(super) fn probe(root: &Path) -> Volume {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    let mut path: Vec<u16> = root.as_os_str().encode_wide().collect();
    path.push(0);
    let mut volume = vec![0_u16; 1024];
    // SAFETY: path is NUL-terminated and volume is writable for its stated length.
    let found = unsafe { GetVolumePathNameW(path.as_ptr(), volume.as_mut_ptr(), 1024) };
    if found == 0 {
        return Volume::UNKNOWN;
    }
    // SAFETY: volume holds a NUL-terminated root path written above.
    let code = unsafe { GetDriveTypeW(volume.as_ptr()) };
    let mut fs_name = vec![0_u16; 64];
    // SAFETY: unused outputs are null; fs_name is writable for its stated length.
    let ok = unsafe {
        GetVolumeInformationW(
            volume.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            fs_name.as_mut_ptr(),
            64,
        )
    };
    let end = fs_name.iter().position(|unit| *unit == 0).unwrap_or(0);
    let text = String::from_utf16_lossy(&fs_name[..end]);
    Volume {
        kind: classify_drive(code),
        fs: if ok == 0 {
            FsClass::Unknown
        } else {
            classify_fs(&text)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_codes_map_to_kinds() {
        assert_eq!(classify_drive(3), DriveKind::Fixed);
        assert_eq!(classify_drive(2), DriveKind::Removable);
        assert_eq!(classify_drive(4), DriveKind::Network);
        assert_eq!(classify_drive(5), DriveKind::Other);
        assert_eq!(classify_drive(1), DriveKind::Unknown);
    }

    #[test]
    fn file_system_names_map_to_classes() {
        assert_eq!(classify_fs("ntfs"), FsClass::Ntfs);
        assert_eq!(classify_fs("ReFS"), FsClass::Refs);
        assert_eq!(classify_fs("FAT32"), FsClass::Fat);
        assert_eq!(classify_fs("exFAT"), FsClass::ExFat);
        assert_eq!(classify_fs("zqxjv-fs"), FsClass::Other);
        assert_eq!(classify_fs(""), FsClass::Unknown);
    }
}
