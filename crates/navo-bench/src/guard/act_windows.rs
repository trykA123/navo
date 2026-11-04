use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path, PathBuf, Prefix};

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    HANDLE, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
    FileDispositionInfoEx, GetDriveTypeW, OPEN_EXISTING, SYNCHRONIZE, SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

use super::act::{Access, Identity, VerifiedItem};
use super::{GuardError, PathRole, RecycleError, Recycler, platform, shell_sink};

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
/// Data access, so share checks apply: nobody can then open the folder with delete access.
const HOLD_ACCESS: u32 = FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
/// No delete sharing: a held folder cannot be renamed, deleted or replaced.
const HOLD_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;

/// Opens the item itself, never a link target; others may still read, write, rename or delete it.
pub(super) fn open_act(path: &Path) -> io::Result<File> {
    open_raw(path, FILE_READ_ATTRIBUTES | DELETE, SHARE_ALL)
}

/// Holds a folder by path so that nobody can rename or delete it while the handle lives.
pub(super) fn hold_dir(path: &Path) -> io::Result<File> {
    open_raw(path, HOLD_ACCESS, HOLD_SHARE)
}

/// Holds the folder `name` inside `parent`, opened relative to that handle, never through a link.
pub(super) fn hold_dir_at(parent: &File, name: &OsStr) -> io::Result<File> {
    let open = NtOpen {
        access: HOLD_ACCESS,
        share: HOLD_SHARE,
        disposition: FILE_OPEN,
        options: FILE_DIRECTORY_FILE,
    };
    nt_open(parent, name, &open)
}

/// Opens the item `name` inside `parent` itself, never a link target; others keep full sharing.
pub(super) fn open_act_at(parent: &File, name: &OsStr, access: Access) -> io::Result<File> {
    let access = match access {
        Access::Inspect => FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        Access::Delete => FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
    };
    let open = NtOpen {
        access,
        share: SHARE_ALL,
        disposition: FILE_OPEN,
        options: 0,
    };
    nt_open(parent, name, &open)
}

/// Creates a new file `name` inside `parent`; an existing entry of any kind fails the call.
pub(super) fn create_file_at(parent: &File, name: &OsStr) -> io::Result<File> {
    let open = NtOpen {
        access: FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE,
        share: SHARE_ALL,
        disposition: FILE_CREATE,
        options: FILE_NON_DIRECTORY_FILE,
    };
    nt_open(parent, name, &open)
}

/// Creates a new folder `name` inside `parent` and returns the handle of exactly that folder.
pub(super) fn create_dir_at(parent: &File, name: &OsStr) -> io::Result<File> {
    let open = NtOpen {
        access: FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
        share: SHARE_ALL,
        disposition: FILE_CREATE,
        options: FILE_DIRECTORY_FILE,
    };
    nt_open(parent, name, &open)
}

/// Creates a new folder `name` inside `parent` and holds it from that moment: no delete sharing,
/// so nobody else can rename or delete it, while the handle itself can still delete it.
pub(super) fn create_held_dir_at(parent: &File, name: &OsStr) -> io::Result<File> {
    let open = NtOpen {
        access: HOLD_ACCESS | DELETE,
        share: HOLD_SHARE,
        disposition: FILE_CREATE,
        options: FILE_DIRECTORY_FILE,
    };
    nt_open(parent, name, &open)
}

struct NtOpen {
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
}

/// One `NtCreateFile` of a single name relative to `parent`; no path is parsed again.
fn nt_open(parent: &File, name: &OsStr, open: &NtOpen) -> io::Result<File> {
    let mut wide: Vec<u16> = name.encode_wide().collect();
    if wide.is_empty()
        || wide
            .iter()
            .any(|unit| matches!(unit, 0 | 0x2F | 0x3A | 0x5C))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let bytes = wide
        .len()
        .checked_mul(2)
        .and_then(|bytes| u16::try_from(bytes).ok())
        .ok_or(io::ErrorKind::InvalidInput)?;
    let object = UNICODE_STRING {
        Length: bytes,
        MaximumLength: bytes,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>())
            .map_err(|_| io::ErrorKind::InvalidInput)?,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &raw const object,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: every pointer is valid for the call; `wide` and `object` outlive it; `parent` is a
    // live directory handle and the name is one component relative to it.
    let result = unsafe {
        NtCreateFile(
            &raw mut handle,
            open.access,
            &raw const attributes,
            &raw mut status,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            open.share,
            open.disposition,
            open.options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if result < 0 {
        // SAFETY: a pure translation of a status code.
        let code = unsafe { RtlNtStatusToDosError(result) };
        return Err(io::Error::from_raw_os_error(code.cast_signed()));
    }
    // SAFETY: NtCreateFile succeeded and returned a fresh handle that nothing else owns.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// Holds the run root without delete sharing, so nobody can rename, delete or replace it.
pub(super) fn pin_root(path: &Path) -> io::Result<File> {
    let access = FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES;
    open_raw(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE)
}

fn open_raw(path: &Path, access: u32, share: u32) -> io::Result<File> {
    let wide = wide_nul(path.as_os_str())?;
    // SAFETY: `wide` is NUL-terminated and outlives the call; no security attributes or template.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            share,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned a fresh valid handle that nothing else owns.
    Ok(unsafe { File::from_raw_handle(handle) })
}

pub(super) fn wide_nul(text: &std::ffi::OsStr) -> io::Result<Vec<u16>> {
    let wide: Vec<u16> = text.encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(wide.into_iter().chain(Some(0)).collect())
}

pub(super) fn is_reparse(handle: &File, role: PathRole) -> Result<bool, GuardError> {
    let metadata = handle.metadata().map_err(|_| GuardError::Io(role))?;
    Ok(platform::is_reparse(&metadata))
}

pub(super) fn final_path(handle: &File, role: PathRole) -> Result<PathBuf, GuardError> {
    platform::final_path(handle, Path::new(""), role)
}

pub(super) fn identity(handle: &File, role: PathRole) -> Result<Identity, GuardError> {
    let (volume, file) = platform::identity(handle).map_err(|_| GuardError::Io(role))?;
    Ok(Identity {
        volume,
        file: u128::from_le_bytes(file),
    })
}

/// Identity of whatever `path` names now, without following a final link.
pub(super) fn identity_at(path: &Path) -> io::Result<Option<Identity>> {
    match open_raw(path, FILE_READ_ATTRIBUTES, SHARE_ALL) {
        Ok(handle) => identity(&handle, PathRole::ScratchRoot)
            .map(Some)
            .map_err(|_| io::Error::other("identity")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Deletes through the verified handle itself, never by path.
pub(super) fn delete(item: &VerifiedItem, role: PathRole) -> Result<(), GuardError> {
    delete_handle(item.handle(), role)
}

/// Deletes the item a fresh create made, through its own handle; a folder must be empty.
pub(super) fn remove_stray(handle: &File, role: PathRole) -> Result<(), GuardError> {
    delete_handle(handle, role)
}

/// Deletes the item a relative create made, through the handle that create returned.
pub(super) fn remove_stray_at(
    _parent: &File,
    _name: &OsStr,
    handle: &File,
    role: PathRole,
) -> Result<(), GuardError> {
    delete_handle(handle, role)
}

/// An attribute-only handle on what `path` names now, never a link target; it blocks nothing.
pub(super) fn open_attributes(path: &Path) -> io::Result<File> {
    open_raw(path, FILE_READ_ATTRIBUTES, SHARE_ALL)
}

fn delete_handle(handle: &File, role: PathRole) -> Result<(), GuardError> {
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    let size =
        u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>()).map_err(|_| GuardError::Io(role))?;
    // SAFETY: the handle is live and `info` is valid for `size` bytes; no DELETE access fails cleanly.
    let done = unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            FileDispositionInfoEx,
            std::ptr::from_ref(&info).cast(),
            size,
        )
    };
    if done == 0 {
        return Err(GuardError::Io(role));
    }
    Ok(())
}

/// Recycles through `IFileOperation`; its sink vetoes any delete that is not one Recycle Bin move
/// of the verified item, reopened without following links. Every folder from the container down
/// to the item's parent is held without delete sharing until the shell has finished.
/// Residual: the shell acts by path, so the item itself (never a held folder) can be replaced by a
/// same-named item in the same held parent between the veto's check and the shell's own open.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShellRecycler;

impl super::act::sealed::Sealed for ShellRecycler {}

impl Recycler for ShellRecycler {
    fn recycle(&self, item: &VerifiedItem) -> Result<(), RecycleError> {
        let (letter, shell_path) = shell_path(item.path())?;
        require_fixed(letter)?;
        shell_sink::recycle(&shell_path, item.identity())
    }
}

/// The handle's final path without the `\\?\` prefix, which the shell does not parse.
fn shell_path(path: &Path) -> Result<(u8, PathBuf), RecycleError> {
    let mut parts = path.components();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        return Err(RecycleError::Io);
    };
    let letter = match prefix.kind() {
        Prefix::VerbatimDisk(letter) | Prefix::Disk(letter) => letter,
        _ => return Err(RecycleError::NotFixedDrive),
    };
    let mut out = PathBuf::from(format!("{}:\\", char::from(letter)));
    for part in parts {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => out.push(name),
            _ => return Err(RecycleError::Io),
        }
    }
    Ok((letter, out))
}

fn require_fixed(letter: u8) -> Result<(), RecycleError> {
    let root: Vec<u16> = format!("{}:\\\0", char::from(letter))
        .encode_utf16()
        .collect();
    // SAFETY: `root` is a NUL-terminated drive root that outlives the call.
    if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
        return Err(RecycleError::NotFixedDrive);
    }
    Ok(())
}
