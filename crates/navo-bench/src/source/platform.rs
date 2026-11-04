use std::fs::{self, DirEntry, File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

use super::{EntryKind, SourceError, unavailable};

pub(super) fn check_directory(path: &Path) -> Result<(), SourceError> {
    let metadata = fs::symlink_metadata(path).map_err(unavailable)?;
    if is_link(&metadata) {
        return Err(SourceError::Link);
    }
    if !metadata.is_dir() {
        return Err(SourceError::NotDirectory);
    }
    Ok(())
}

pub(super) fn check_ancestors(root: &Path, path: &Path) -> Result<(), SourceError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| SourceError::ForeignId)?;
    check_directory(root)?;
    let mut current = root.to_path_buf();
    let mut parts = relative.components().peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        let metadata = fs::symlink_metadata(&current).map_err(unavailable)?;
        if is_link(&metadata) {
            return Err(SourceError::Link);
        }
        if parts.peek().is_some() && !metadata.is_dir() {
            return Err(SourceError::NotDirectory);
        }
        if parts.peek().is_none() && !metadata.is_file() {
            return Err(SourceError::NotFile);
        }
    }
    Ok(())
}

/// The item's own record, not the listing's copy: NTFS refreshes a name's copy only when that
/// name is opened, so a write through another hard link can stay hidden from the listing.
#[cfg(windows)]
pub(super) fn entry_metadata(entry: &DirEntry) -> io::Result<Metadata> {
    fs::symlink_metadata(entry.path())
}

#[cfg(not(windows))]
pub(super) fn entry_metadata(entry: &DirEntry) -> io::Result<Metadata> {
    entry.metadata()
}

#[cfg(windows)]
pub(super) fn is_link(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn is_link(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(super) fn kind(metadata: &Metadata, _path: &Path) -> EntryKind {
    if is_link(metadata) {
        #[cfg(windows)]
        return reparse_kind(_path).unwrap_or(EntryKind::ReparsePoint);
        #[cfg(not(windows))]
        return EntryKind::SymbolicLink;
    }
    if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "reparse classification queries a read-only, no-follow handle"
)]
#[expect(
    clippy::disallowed_methods,
    reason = "attribute-only, no-follow open shares read, write and delete"
)]
fn reparse_kind(path: &Path) -> Option<EntryKind> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_TAG_INFO, FILE_READ_ATTRIBUTES, FileAttributeTagInfo,
        GetFileInformationByHandleEx,
    };
    let file = options()
        .access_mode(FILE_READ_ATTRIBUTES)
        .open(path)
        .ok()?;
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    let size = u32::try_from(std::mem::size_of_val(&info)).ok()?;
    // SAFETY: file is live and info is a writable buffer of the exact requested size.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&raw mut info).cast(),
            size,
        )
    };
    if ok == 0 {
        return None;
    }
    Some(match info.ReparseTag {
        0xA000_0003 => EntryKind::Junction,
        0xA000_000C => EntryKind::SymbolicLink,
        _ => EntryKind::ReparsePoint,
    })
}

#[cfg(windows)]
pub(super) fn attributes(metadata: &Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes()
}

#[cfg(unix)]
pub(super) fn attributes(metadata: &Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode()
}

#[cfg(windows)]
pub(super) fn hard_links(_metadata: &Metadata) -> Option<u64> {
    None
}

#[cfg(unix)]
pub(super) fn hard_links(metadata: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.nlink())
}

#[cfg(unix)]
pub(super) fn file_hard_links(file: &File) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(file.metadata()?.nlink())
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "link count read from a live handle into an owned struct"
)]
pub(super) fn file_hard_links(file: &File) -> io::Result<u64> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is live and info is a writable struct of the expected type.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut info) };
    if ok == 0 {
        return Err(io::Error::from(io::Error::last_os_error().kind()));
    }
    Ok(u64::from(info.nNumberOfLinks))
}

#[cfg(windows)]
fn options() -> OpenOptions {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options
}

#[cfg(unix)]
fn options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    options
}

#[expect(
    clippy::disallowed_methods,
    reason = "read-only, no-follow opens with sharing all on Windows"
)]
pub(super) fn open_file(path: &Path) -> io::Result<File> {
    options().open(path)
}

#[expect(
    clippy::disallowed_methods,
    reason = "read-only directory open with sharing all on Windows"
)]
pub(super) fn open_directory(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    let options = {
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = options();
        options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
        options
    };
    #[cfg(windows)]
    let options = options();
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    #[cfg(windows)]
    if !same_final_path(&file, path)? {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(file)
}

/// True when the handle's final path is `path`; a folder swapped for a junction fails this.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "final-path lookup reads a live handle into an owned buffer"
)]
fn same_final_path(file: &File, path: &Path) -> io::Result<bool> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };
    let mut buffer = vec![0_u16; 32768];
    let capacity = u32::try_from(buffer.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: the live handle and the writable buffer of `capacity` units outlive the call.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            capacity,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if length == 0 || length >= capacity {
        return Err(io::Error::last_os_error());
    }
    buffer.truncate(usize::try_from(length).map_err(|_| io::ErrorKind::InvalidInput)?);
    let expected: Vec<u16> = path.as_os_str().encode_wide().collect();
    let (left, right) = (verbatim_tail(&buffer), verbatim_tail(&expected));
    let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return Ok(false);
    };
    // SAFETY: both slices hold the stated number of UTF-16 units.
    let order =
        unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) };
    Ok(order == CSTR_EQUAL)
}

#[cfg(windows)]
fn verbatim_tail(units: &[u16]) -> &[u16] {
    const SLASH: u16 = 0x5C;
    const VERBATIM: [u16; 4] = [SLASH, SLASH, 0x3F, SLASH];
    let tail = units.strip_prefix(&VERBATIM[..]).unwrap_or(units);
    match tail.strip_suffix(&[SLASH]) {
        Some(trimmed) if !trimmed.ends_with(&[0x3A]) => trimmed,
        _ => tail,
    }
}
