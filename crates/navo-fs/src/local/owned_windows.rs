use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_INFO_BY_HANDLE_CLASS, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FileBasicInfo,
    FileDispositionInfo, FileDispositionInfoEx, GetFileInformationByHandleEx,
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

/// Deletes the moved source at `path` only while it is still `expected` (see
/// [`crate::CopiedSource::still`]), checked and deleted through one handle, so a file put there
/// meanwhile is never deleted. `Ok(false)` when it changed.
///
/// The handle shares no write access: while another program holds the file open for writing the
/// open fails with a sharing violation and the source stays (reported as in use), and no writer
/// can open it between the check and the delete. NTFS can defer a modified-time update until a
/// writer closes its handle; refusing write sharing is what closes that gap here.
pub(super) fn remove_unchanged(path: &Path, expected: crate::CopiedSource) -> io::Result<bool> {
    let file = match open_source(path, FILE_WRITE_ATTRIBUTES) {
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED.cast_signed()) => {
            open_source(path, 0)?
        }
        other => other?,
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || !expected.still(&super::copied_source(&file, &metadata)?) {
        return Ok(false);
    }
    match dispose_posix(&file) {
        Err(error) if rejects_ex(&error) => dispose_writable(&file),
        other => other,
    }
    .map(|()| true)
}

/// The file at `path` as a move's source proof, read through one handle opened by path with
/// attribute access only, so creation time and attribute bits come from the same file as size and
/// times. `None` when it is not a plain file (a link is not followed) or any open or read fails:
/// that proves nothing, and a rollback keeps its copy.
pub(super) fn source_at(path: &Path) -> Option<crate::CopiedSource> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    super::copied_source(&file, &metadata).ok()
}

/// Opens a moved source for its check and delete, with `extra` access (write attributes, so a
/// read-only source can be cleared on this handle); never shares write access.
fn open_source(path: &Path, extra: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES | extra)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

/// The classic disposition refuses a read-only file: clear the flag on this checked handle first
/// and put it back when the delete still fails. Without write-attribute access the source stays.
fn dispose_writable(file: &File) -> io::Result<()> {
    let info = basic(file)?;
    if info.FileAttributes & FILE_ATTRIBUTE_READONLY == 0 {
        return dispose(file);
    }
    let mut writable = info;
    writable.FileAttributes &= !FILE_ATTRIBUTE_READONLY;
    if writable.FileAttributes == 0 {
        writable.FileAttributes = FILE_ATTRIBUTE_NORMAL;
    }
    set_basic(file, &writable)?;
    dispose(file).inspect_err(|_| {
        let _ = set_basic(file, &info);
    })
}

/// True when the volume (FAT, SMB) does not take the `FileDispositionInfoEx` form.
fn rejects_ex(error: &io::Error) -> bool {
    [
        ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED,
        ERROR_INVALID_FUNCTION,
    ]
    .iter()
    .any(|code| error.raw_os_error() == Some(code.cast_signed()))
}

fn dispose_posix(file: &File) -> io::Result<()> {
    set_info(
        file,
        &FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        },
    )
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

fn set_basic(file: &File, info: &FILE_BASIC_INFO) -> io::Result<()> {
    set_info(file, info)
}

fn dispose(file: &File) -> io::Result<()> {
    set_info(file, &FILE_DISPOSITION_INFO { DeleteFile: true })
}

/// A handle information struct paired with the class that `SetFileInformationByHandle` expects.
trait HandleInfo {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS;
}

impl HandleInfo for FILE_BASIC_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileBasicInfo;
}

impl HandleInfo for FILE_DISPOSITION_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileDispositionInfo;
}

impl HandleInfo for FILE_DISPOSITION_INFO_EX {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileDispositionInfoEx;
}

/// Updates `file` through its live handle with `info`, whose class matches its type.
#[allow(
    unsafe_code,
    reason = "owned Windows metadata and deletion go through the verified handle"
)]
fn set_info<T: HandleInfo>(file: &File, info: &T) -> io::Result<()> {
    // SAFETY: the live handle and initialized input buffer survive the update, and
    // `HandleInfo` pairs the buffer type with its information class.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            T::CLASS,
            std::ptr::from_ref(info).cast(),
            std::mem::size_of::<T>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
