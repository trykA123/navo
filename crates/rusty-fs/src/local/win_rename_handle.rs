#![allow(
    unsafe_code,
    reason = "std has no rename through an already open handle"
)]

use std::fs::{File, OpenOptions};
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH,
    FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FileRenameInfo,
    SetFileInformationByHandle,
};

pub(super) fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
        )
        .open(path)
}

pub(super) fn rename(file: &File, to: &Path) -> io::Result<()> {
    let name: Vec<u16> = to.as_os_str().encode_wide().collect();
    let name_bytes = name
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(too_long)?;
    let length = u32::try_from(name_bytes).map_err(|_| too_long())?;
    let size = size_of::<FILE_RENAME_INFO>()
        .checked_add(name_bytes)
        .ok_or_else(too_long)?;
    let buffer_size = u32::try_from(size).map_err(|_| too_long())?;
    let mut buffer = vec![0_u64; size.div_ceil(size_of::<u64>())];
    let base = buffer.as_mut_ptr().cast::<u8>();
    let info = base.cast::<FILE_RENAME_INFO>();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    // SAFETY: the zeroed, u64-aligned buffer holds the header, the name and its NUL, so
    // ReplaceIfExists is false, RootDirectory is null and every write stays inside it.
    unsafe {
        std::ptr::addr_of_mut!((*info).FileNameLength).write(length);
        let at = base.add(offset).cast::<u16>();
        std::ptr::copy_nonoverlapping(name.as_ptr(), at, name.len());
    }
    // SAFETY: the live handle was opened with DELETE access and the buffer outlives the call.
    let renamed = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            buffer.as_ptr().cast(),
            buffer_size,
        )
    };
    if renamed == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn too_long() -> io::Error {
    io::Error::from(io::ErrorKind::InvalidInput)
}
