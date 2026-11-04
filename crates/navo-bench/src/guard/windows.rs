use std::ffi::OsStr;
use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf, Prefix};

use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FileIdInfo,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
};

use super::{GuardError, PathRole, resolve};

/// Attribute-only handle for identity and final-path checks. It pins nothing: share checks
/// ignore FILE_READ_ATTRIBUTES opens, so others may still rename or delete the item.
#[allow(
    clippy::disallowed_methods,
    reason = "read-attributes-only open of an existing item, no data access"
)]
pub(super) fn pin(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

pub(super) fn final_path(
    handle: &File,
    _path: &Path,
    role: PathRole,
) -> Result<PathBuf, GuardError> {
    let mut buffer = vec![0_u16; 32768];
    let capacity = u32::try_from(buffer.len()).map_err(|_| GuardError::InvalidPath(role))?;
    // SAFETY: the live file handle and writable buffer remain valid throughout the call.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            handle.as_raw_handle(),
            buffer.as_mut_ptr(),
            capacity,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if length == 0 || length >= capacity {
        return Err(GuardError::Io(role));
    }
    let length = usize::try_from(length).map_err(|_| GuardError::InvalidPath(role))?;
    buffer.truncate(length);
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)))
}

pub(super) fn is_reparse(metadata: &Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

pub(super) fn valid_component(component: Component<'_>) -> bool {
    match component {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) => true,
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                valid_name(server) && valid_name(share)
            }
            _ => false,
        },
        Component::Normal(name) => valid_name(name),
        Component::ParentDir | Component::CurDir => false,
        Component::RootDir => true,
    }
}

fn valid_name(name: &OsStr) -> bool {
    let units: Vec<u16> = name.encode_wide().collect();
    if units.is_empty() || units.last().is_some_and(|c| matches!(c, 32 | 46)) {
        return false;
    }
    if units
        .iter()
        .any(|c| *c < 32 || [34, 42, 47, 58, 60, 62, 63, 92, 124].contains(c))
    {
        return false;
    }
    let base: Vec<u16> = units.iter().copied().take_while(|c| *c != 46).collect();
    if base.last() == Some(&32) {
        return false;
    }
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    if reserved
        .iter()
        .any(|word| equal(OsStr::new(word), &std::ffi::OsString::from_wide(&base)))
    {
        return false;
    }
    if base.len() == 4 && [49, 50, 51, 52, 53, 54, 55, 56, 57, 185, 178, 179].contains(&base[3]) {
        let prefix = std::ffi::OsString::from_wide(&base[..3]);
        return !equal(&prefix, OsStr::new("COM")) && !equal(&prefix, OsStr::new("LPT"));
    }
    true
}

pub(super) fn equal(left: &OsStr, right: &OsStr) -> bool {
    let left: Vec<u16> = left.encode_wide().collect();
    let right: Vec<u16> = right.encode_wide().collect();
    let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return false;
    };
    // SAFETY: both buffers contain the specified number of UTF-16 code units.
    unsafe {
        CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) == CSTR_EQUAL
    }
}

pub(super) fn sync_directory(_handle: &File) -> io::Result<()> {
    Ok(())
}

pub(super) fn verify_root(
    root: &resolve::Resolved,
    candidate: &resolve::Resolved,
) -> Result<(), GuardError> {
    let role = PathRole::ScratchRoot;
    let ancestor = candidate
        .path
        .ancestors()
        .find(|path| resolve::equal(path, &root.path))
        .ok_or(GuardError::OutsideRoot(role))?;
    let candidate_handle = pin(ancestor).map_err(|_| GuardError::Io(role))?;
    let root_handle = root._handles.last().ok_or(GuardError::Changed(role))?;
    let candidate_id = identity(&candidate_handle)?;
    let root_id = identity(root_handle)?;
    if candidate_id != root_id {
        return Err(GuardError::OutsideRoot(role));
    }
    Ok(())
}

pub(super) fn identity(handle: &File) -> Result<(u64, [u8; 16]), GuardError> {
    let mut info = FILE_ID_INFO::default();
    let size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .map_err(|_| GuardError::Changed(PathRole::ScratchRoot))?;
    // SAFETY: the live handle and initialized output structure are valid for this call.
    let success = unsafe {
        GetFileInformationByHandleEx(
            handle.as_raw_handle(),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    if success == 0 {
        return Err(GuardError::Io(PathRole::ScratchRoot));
    }
    Ok((info.VolumeSerialNumber, info.FileId.Identifier))
}
