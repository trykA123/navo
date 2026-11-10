//! Windows chain pin: holds created folders, rechecks them, and removes only ours.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "folder holds, file ids and delete disposition require Win32 FFI"
)]

use std::ffi::c_void;
use std::fmt;
use std::io;
use std::mem::size_of;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileAttributeTagInfo, FileDispositionInfo, FileIdInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING, SYNCHRONIZE, SetFileInformationByHandle,
};

use crate::placement::{Chain, FileId};
use crate::probe::chain::resolve_path;

const HOLD_ACCESS: u32 = FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE;
const HOLD_FLAGS: u32 = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT;

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;

/// A failure while creating or holding a folder.
#[derive(Debug)]
pub enum PinError {
    InvalidChain,
    RootExists,
    Identity,
    NotFolder,
    Reparse,
    Io(io::Error),
}

impl fmt::Display for PinError {
    /// Describes the pin failure without exposing paths.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidChain => formatter.write_str("pin chain has no existing parent"),
            Self::RootExists => formatter.write_str("pin root already exists"),
            Self::Identity => formatter.write_str("pin folder identity changed"),
            Self::NotFolder => formatter.write_str("pin path is not a folder"),
            Self::Reparse => formatter.write_str("pin folder is a reparse point"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for PinError {}

impl From<io::Error> for PinError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// The folders removed and kept by a cleanup.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Removal {
    pub removed: Vec<PathBuf>,
    pub kept: Vec<PathBuf>,
}

/// Holds the root and every folder this session created until cleanup or close.
pub struct Pin {
    existing_root: Option<Held>,
    created: Vec<Held>,
}

impl Pin {
    /// Creates and holds every missing folder in `chain`; a partial failure cleans up first.
    pub fn fresh(chain: &Chain) -> Result<Self, PinError> {
        let Some(base) = chain.existing.last() else {
            return Err(PinError::InvalidChain);
        };
        if chain.missing.is_empty() {
            let Some(root) = chain.existing.last() else {
                return Err(PinError::InvalidChain);
            };
            let handle = open_hold(&root.path)?;
            let (id, reparse, directory) = read_id(&handle)?;
            if reparse {
                return Err(PinError::Reparse);
            }
            if !directory {
                return Err(PinError::NotFolder);
            }
            if id != root.id {
                return Err(PinError::Identity);
            }
            if !crate::host::uninstall_leftovers_only(&root.path)? {
                return Err(PinError::RootExists);
            }
            return Ok(Self {
                existing_root: Some(Held {
                    path: root.path.clone(),
                    id,
                    handle: Some(handle),
                }),
                created: Vec::new(),
            });
        }
        let mut created = Vec::new();
        let mut path = base.path.clone();
        for name in &chain.missing {
            path.push(name);
            match create_and_hold(&path) {
                Ok(held) => created.push(held),
                Err(error) => {
                    remove_folders(created);
                    return Err(error);
                }
            }
        }
        Ok(Self {
            existing_root: None,
            created,
        })
    }

    /// Holds an existing root after checking its identity and reparse flag.
    pub fn existing(root: &Path, expected: FileId) -> Result<Self, PinError> {
        let handle = open_hold(root)?;
        let (id, reparse, directory) = read_id(&handle)?;
        if reparse {
            return Err(PinError::Reparse);
        }
        if !directory {
            return Err(PinError::NotFolder);
        }
        if id != expected {
            return Err(PinError::Identity);
        }
        Ok(Self {
            existing_root: Some(Held {
                path: root.to_path_buf(),
                id,
                handle: Some(handle),
            }),
            created: Vec::new(),
        })
    }

    /// Whether the root is one Setup created in this session.
    pub fn created_root(&self) -> bool {
        self.existing_root.is_none() && !self.created.is_empty()
    }

    /// The held root path, or `None` when the pin holds nothing.
    pub fn root(&self) -> Option<&Path> {
        self.existing_root
            .as_ref()
            .or_else(|| self.created.last())
            .map(|held| held.path.as_path())
    }

    /// Re-reads every held folder through its handle.
    pub fn unchanged(&self) -> bool {
        self.existing_root.as_ref().is_none_or(held_ok) && self.created.iter().all(held_ok)
    }

    /// Deletes only the folders this session created, deepest first, stopping at the first keep.
    pub fn remove_created(self) -> Removal {
        let Pin {
            mut existing_root,
            created,
        } = self;
        if let Some(root) = &mut existing_root {
            root.close();
        }
        remove_folders(created)
    }

    /// Test hook: closes held handles while keeping the recorded paths and ids.
    #[cfg(test)]
    pub(crate) fn close_handles(&mut self) {
        if let Some(root) = &mut self.existing_root {
            root.close();
        }
        for held in &mut self.created {
            held.close();
        }
    }
}

/// Re-reads the existing ancestors by path and checks identity and reparse state.
pub fn ancestors_unchanged(chain: &Chain) -> bool {
    let Some(last) = chain.existing.last() else {
        return false;
    };
    let Ok(current) = resolve_path(&last.path) else {
        return false;
    };
    current.missing.is_empty()
        && current.existing.len() == chain.existing.len()
        && current
            .existing
            .iter()
            .zip(&chain.existing)
            .all(|(now, then)| !now.reparse && !then.reparse && now.id == then.id)
}

/// One held folder: its path, identity and optional open handle.
struct Held {
    path: PathBuf,
    id: FileId,
    handle: Option<Owned>,
}

impl Held {
    /// Closes the hold handle; a later cleanup can reopen by path.
    fn close(&mut self) {
        self.handle = None;
    }

    /// Re-reads the held folder through its handle.
    fn read(&self) -> io::Result<(FileId, bool)> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| io::Error::other("pin handle closed"))?;
        let (id, reparse, _) = read_id(handle)?;
        Ok((id, reparse))
    }
}

/// Whether a held folder still has its recorded identity and no reparse tag.
fn held_ok(held: &Held) -> bool {
    held.read()
        .is_ok_and(|(id, reparse)| !reparse && id == held.id)
}

/// Creates one folder and immediately holds it.
fn create_and_hold(path: &Path) -> Result<Held, PinError> {
    let wide = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and no security attributes are used.
    let created = unsafe { CreateDirectoryW(wide.as_ptr(), std::ptr::null()) };
    if created == 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::AlreadyExists {
            Err(PinError::RootExists)
        } else {
            Err(PinError::Io(error))
        };
    }
    let handle = open_hold(path)?;
    let (id, reparse, directory) = read_id(&handle)?;
    if reparse {
        return Err(PinError::Reparse);
    }
    if !directory {
        return Err(PinError::NotFolder);
    }
    Ok(Held {
        path: path.to_path_buf(),
        id,
        handle: Some(handle),
    })
}

/// Opens a folder with the IW7a hold access, flags and share mode.
fn open_hold(path: &Path) -> io::Result<Owned> {
    let wide = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and the buffers are live for the call.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            HOLD_ACCESS,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            HOLD_FLAGS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Owned(handle))
}

/// Reads the identity, reparse flag and directory flag through a live handle.
fn read_id(handle: &Owned) -> io::Result<(FileId, bool, bool)> {
    let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
    let tag_size = u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>()).map_err(io::Error::other)?;
    let tag_ptr = std::ptr::from_mut(&mut tag).cast::<c_void>();
    // SAFETY: the handle is live and `tag` is a writable struct of the size passed.
    let tag_ok =
        unsafe { GetFileInformationByHandleEx(handle.0, FileAttributeTagInfo, tag_ptr, tag_size) };
    if tag_ok == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut info = FILE_ID_INFO::default();
    let id_size = u32::try_from(size_of::<FILE_ID_INFO>()).map_err(io::Error::other)?;
    let info_ptr = std::ptr::from_mut(&mut info).cast::<c_void>();
    // SAFETY: the same live handle is used and `info` is a writable struct of the size passed.
    let id_ok = unsafe { GetFileInformationByHandleEx(handle.0, FileIdInfo, info_ptr, id_size) };
    if id_ok == 0 {
        return Err(io::Error::last_os_error());
    }

    let reparse = tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0;
    let directory = tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    let id = FileId {
        volume: info.VolumeSerialNumber,
        id: u128::from_le_bytes(info.FileId.Identifier),
    };
    Ok((id, reparse, directory))
}

/// Opens a created folder for deletion, without following a reparse point.
fn open_for_delete(path: &Path) -> io::Result<Owned> {
    let wide = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and the buffers are live for the call.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            DELETE | FILE_READ_ATTRIBUTES,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            HOLD_FLAGS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Owned(handle))
}

/// Deletes one recorded empty folder; a changed id, reparse or non-empty folder keeps it.
fn delete_created(path: &Path, expected: FileId) -> bool {
    let Ok(handle) = open_for_delete(path) else {
        return false;
    };
    let Ok((id, reparse, directory)) = read_id(&handle) else {
        return false;
    };
    if reparse || !directory || id != expected {
        return false;
    }
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let info_ptr = std::ptr::from_ref(&info).cast::<c_void>();
    let Ok(size) = u32::try_from(size_of::<FILE_DISPOSITION_INFO>()) else {
        return false;
    };
    // SAFETY: the handle is live and `info` is a readable struct of the size passed.
    let ok = unsafe { SetFileInformationByHandle(handle.0, FileDispositionInfo, info_ptr, size) };
    if ok == 0 {
        return false;
    }
    drop(handle);
    std::fs::symlink_metadata(path).is_err()
}

/// Runs the cleanup over `created`, deepest first, stopping at the first kept folder.
fn remove_folders(mut created: Vec<Held>) -> Removal {
    created.reverse();
    let mut removal = Removal::default();
    let mut keep_parents = false;
    for mut held in created {
        let path = held.path.clone();
        if keep_parents {
            removal.kept.push(path);
            continue;
        }
        held.close();
        if delete_created(&path, held.id) {
            removal.removed.push(path);
        } else {
            removal.kept.push(path);
            keep_parents = true;
        }
    }
    removal
}

/// A NUL-terminated UTF-16 path for the Win32 folder calls.
fn wide_z(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A Win32 handle that closes on drop.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this guard owns a live handle that nothing else closes.
        unsafe { CloseHandle(self.0) };
    }
}
