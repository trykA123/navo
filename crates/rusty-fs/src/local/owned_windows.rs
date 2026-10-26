use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_DISPOSITION_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_WRITE_ATTRIBUTES, FileBasicInfo, FileDispositionInfo, GetFileInformationByHandleEx,
    SetFileInformationByHandle,
};

pub(super) fn remove(path: &Path, expected: crate::FileIdentity) -> io::Result<()> {
    let file = OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if super::identity::from_file(&file)? != Some(expected) {
        return Err(io::Error::other("temporary file identity changed"));
    }
    let mut info = basic(&file)?;
    if info.FileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(io::Error::other("temporary path is not a regular file"));
    }
    if info.FileAttributes & FILE_ATTRIBUTE_READONLY != 0 {
        info.FileAttributes &= !FILE_ATTRIBUTE_READONLY;
        if info.FileAttributes == 0 {
            info.FileAttributes = FILE_ATTRIBUTE_NORMAL;
        }
        set_basic(&file, &info)?;
    }
    dispose(&file)
}

#[allow(
    unsafe_code,
    reason = "owned Windows metadata uses the live file handle"
)]
fn basic(file: &File) -> io::Result<FILE_BASIC_INFO> {
    let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::zeroed();
    // SAFETY: the live handle and correctly sized writable output buffer survive the query.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a successful native query initializes FILE_BASIC_INFO.
    Ok(unsafe { info.assume_init() })
}

#[allow(
    unsafe_code,
    reason = "owned Windows metadata is updated through its verified handle"
)]
fn set_basic(file: &File, info: &FILE_BASIC_INFO) -> io::Result<()> {
    // SAFETY: the live handle and initialized input buffer survive the update.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileBasicInfo,
            std::ptr::from_ref(info).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[allow(
    unsafe_code,
    reason = "owned Windows deletion must target the verified handle"
)]
fn dispose(file: &File) -> io::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the live DELETE-capable handle and initialized input buffer survive the update.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&raw const info).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
