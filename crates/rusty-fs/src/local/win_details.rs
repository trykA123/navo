#![allow(
    unsafe_code,
    reason = "Windows details require native attribute and allocation queries"
)]

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_FUNCTION, ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_STANDARD_INFO,
    FileAttributeTagInfo, FileStandardInfo, GetFileAttributesExW, GetFileExInfoStandard,
    GetFileInformationByHandleEx, WIN32_FILE_ATTRIBUTE_DATA,
};

use super::win_list::{kind_of, modified_ms};
use super::windows::file_attributes;
use crate::{DetailAttributes, EntryKind, FileDetails};

pub(super) fn read(path: &Path) -> io::Result<FileDetails> {
    let data = read_attributes(path)?;
    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let bits = data.dwFileAttributes;
    let kind = kind_of(bits, reparse_tag(&file, bits)?);
    let attributes = file_attributes(bits);
    let is_file = kind == EntryKind::File;
    Ok(FileDetails {
        kind,
        size: if is_file {
            (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow)
        } else {
            0
        },
        on_disk: if is_file {
            allocation_size(&file)?
        } else {
            None
        },
        created_ms: modified_ms(data.ftCreationTime),
        modified_ms: modified_ms(data.ftLastWriteTime),
        accessed_ms: modified_ms(data.ftLastAccessTime),
        attributes: DetailAttributes {
            readonly: attributes.readonly,
            hidden: attributes.hidden,
            system: attributes.system,
            archive: bits & FILE_ATTRIBUTE_ARCHIVE != 0,
        },
        owner: None,
        acl: Vec::new(),
        inherited_from: None,
    })
}

fn read_attributes(path: &Path) -> io::Result<WIN32_FILE_ATTRIBUTE_DATA> {
    let mut name: Vec<u16> = path.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    name.push(0);
    let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
    // SAFETY: the terminated path and correctly sized writable output live through the call.
    let ok = unsafe {
        GetFileAttributesExW(name.as_ptr(), GetFileExInfoStandard, (&raw mut data).cast())
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(data)
}

fn reparse_tag(file: &File, bits: u32) -> io::Result<u32> {
    if bits & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return Ok(0);
    }
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    let bytes = u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>())
        .map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: the live handle and correctly sized writable output remain valid during the call.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&raw mut info).cast(),
            bytes,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info.ReparseTag)
}

fn allocation_size(file: &File) -> io::Result<Option<u64>> {
    let mut info = FILE_STANDARD_INFO::default();
    let bytes =
        u32::try_from(size_of::<FILE_STANDARD_INFO>()).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: the live handle and correctly sized writable output remain valid during the call.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStandardInfo,
            (&raw mut info).cast(),
            bytes,
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error().map(i32::cast_unsigned) {
            Some(
                ERROR_INVALID_FUNCTION
                | ERROR_INVALID_LEVEL
                | ERROR_INVALID_PARAMETER
                | ERROR_NOT_SUPPORTED,
            ) => Ok(None),
            _ => Err(error),
        };
    }
    u64::try_from(info.AllocationSize)
        .map(Some)
        .map_err(|_| io::ErrorKind::InvalidData.into())
}
