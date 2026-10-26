use std::io;
use std::path::Path;

use crate::{FileIdentity, FileIdentityKind};

#[cfg(unix)]
pub(super) fn read(path: &Path) -> io::Result<Option<FileIdentity>> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path)?;
    Ok(Some(FileIdentity {
        volume: metadata.dev(),
        kind: FileIdentityKind::Native,
        file: u128::from(metadata.ino()).to_le_bytes(),
    }))
}

#[cfg(windows)]
pub(super) fn read(path: &Path) -> io::Result<Option<FileIdentity>> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    from_file(&file)
}

#[cfg(windows)]
pub(super) fn from_file(file: &std::fs::File) -> io::Result<Option<FileIdentity>> {
    proved(filesystem_name(file), || file_id(file))
}

#[cfg(any(windows, test))]
fn proved(
    filesystem: io::Result<String>,
    query: impl FnOnce() -> io::Result<Option<FileIdentity>>,
) -> io::Result<Option<FileIdentity>> {
    if stable_ids(&filesystem.unwrap_or_default()) {
        query()
    } else {
        Ok(None)
    }
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "Windows file identity requires a handle query")]
fn file_id(file: &std::fs::File) -> io::Result<Option<FileIdentity>> {
    use std::mem::{MaybeUninit, size_of};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };

    let mut info = MaybeUninit::<FILE_ID_INFO>::zeroed();
    // SAFETY: the handle and correctly sized output buffer live through the query.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            info.as_mut_ptr().cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return if unsupported(&error) {
            legacy(file)
        } else {
            Err(error)
        };
    }
    // SAFETY: a successful query initializes FILE_ID_INFO.
    let info = unsafe { info.assume_init() };
    if info.FileId.Identifier == [0; 16] {
        return legacy(file);
    }
    Ok(Some(FileIdentity {
        volume: info.VolumeSerialNumber,
        file: info.FileId.Identifier,
        kind: FileIdentityKind::Native,
    }))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the legacy Windows identity is read from the same non-following handle"
)]
fn legacy(file: &std::fs::File) -> io::Result<Option<FileIdentity>> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    // SAFETY: the live handle and writable output buffer remain valid through the query.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return if unsupported(&error) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: a successful call initializes BY_HANDLE_FILE_INFORMATION.
    let info = unsafe { info.assume_init() };
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Ok(legacy_identity(info.dwVolumeSerialNumber, index))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "file IDs are trusted only on filesystems that keep them stable"
)]
fn filesystem_name(file: &std::fs::File) -> io::Result<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW;
    let mut name = [0_u16; 64];
    // SAFETY: the live handle and writable filesystem-name buffer remain valid through the query.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    String::from_utf16(&name[..end])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(any(windows, test))]
fn stable_ids(filesystem: &str) -> bool {
    !matches!(
        filesystem.to_ascii_uppercase().as_str(),
        "" | "FAT" | "FAT12" | "FAT16" | "FAT32" | "EXFAT"
    )
}

#[cfg(windows)]
fn unsupported(error: &io::Error) -> bool {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED)
    )
}

#[cfg(any(windows, test))]
fn legacy_identity(volume: u32, index: u64) -> Option<FileIdentity> {
    (index != 0).then_some(FileIdentity {
        volume: u64::from(volume),
        file: u128::from(index).to_le_bytes(),
        kind: FileIdentityKind::WindowsIndex,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE: FileIdentity = FileIdentity {
        volume: 7,
        kind: FileIdentityKind::Native,
        file: [9; 16],
    };

    #[test]
    fn a_fat_family_or_unknown_filesystem_never_trusts_a_file_id() {
        for name in ["", "FAT", "FAT12", "fat16", "FAT32", "exFAT", "EXFAT"] {
            let identity = proved(Ok(name.to_owned()), || {
                panic!("{name} file IDs must not be queried")
            });
            assert_eq!(identity.unwrap(), None, "{name}");
        }
    }

    #[test]
    fn a_stable_filesystem_keeps_its_file_id_and_query_errors() {
        for name in ["NTFS", "ReFS", "ext4"] {
            assert_eq!(
                proved(Ok(name.to_owned()), || Ok(Some(NATIVE))).unwrap(),
                Some(NATIVE)
            );
        }
        let failed = proved(Ok("NTFS".to_owned()), || {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn an_unreadable_filesystem_name_is_unproved_and_never_queries_the_file_id() {
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::InvalidData] {
            let identity = proved(Err(io::Error::from(kind)), || {
                panic!("{kind:?} must not query the file ID")
            });
            assert_eq!(identity.unwrap(), None, "{kind:?}");
        }
    }

    #[test]
    fn legacy_windows_index_is_lossless_and_has_a_distinct_identity_domain() {
        let legacy = legacy_identity(u32::MAX, u64::MAX).unwrap();
        assert_eq!(legacy.volume, u64::from(u32::MAX));
        assert_eq!(legacy.file, u128::from(u64::MAX).to_le_bytes());
        assert_ne!(
            legacy,
            FileIdentity {
                kind: FileIdentityKind::Native,
                ..legacy
            }
        );
        assert!(legacy_identity(7, 0).is_none());
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn read(_: &Path) -> io::Result<Option<FileIdentity>> {
    Ok(None)
}
