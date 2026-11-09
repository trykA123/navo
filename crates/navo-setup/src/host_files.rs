//! Windows file checks for the runner: regular-file opens, hashing and marker ids.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::mem::size_of;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileAttributeTagInfo, FileIdInfo,
    GetFileInformationByHandleEx,
};

use crate::runner::MarkerId;

/// Opens a path without following its final reparse point and accepts only a regular file.
pub(super) fn open_regular(path: &Path) -> io::Result<Option<File>> {
    let file = match OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    read_tag(&file, &mut tag)?;
    let reparse = tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0;
    let directory = tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    Ok((!reparse && !directory).then_some(file))
}

/// Hashes an open regular file, failing on any read error.
pub(super) fn hash_reader(reader: &mut impl Read) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

/// Reads the open marker handle's volume and 128-bit file id.
pub(super) fn marker_id(file: &File) -> io::Result<MarkerId> {
    let mut info = FILE_ID_INFO::default();
    let size = u32::try_from(size_of::<FILE_ID_INFO>()).map_err(io::Error::other)?;
    // SAFETY: the marker handle is live and `info` is a writable struct of the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(MarkerId {
        volume: info.VolumeSerialNumber,
        id: info.FileId.Identifier,
    })
}

/// Reads one file attribute/tag structure from a live handle.
fn read_tag(file: &File, tag: &mut FILE_ATTRIBUTE_TAG_INFO) -> io::Result<()> {
    let size = u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>()).map_err(io::Error::other)?;
    // SAFETY: the file handle is live and `tag` is a writable struct of the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            std::ptr::from_mut(tag).cast(),
            size,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
