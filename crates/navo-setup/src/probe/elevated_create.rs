//! E5: the elevated helper creates the missing folders relative to the handles it holds, never by
//! path, so a link swapped into the chain after its checks cannot redirect a privileged create.

#![allow(
    unsafe_code,
    reason = "delete disposition on a created folder requires Win32 FFI"
)]

use std::ffi::{OsStr, OsString, c_void};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_FOR_BACKUP_INTENT,
    FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
};
use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_INFO_BY_HANDLE_CLASS,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
    FileDispositionInfo, FileDispositionInfoEx, SYNCHRONIZE, SetFileInformationByHandle,
};

use crate::elevate::{HelperArgs, HelperExit};
use crate::payload::platform::security::{self, Local};
use crate::placement::{Chain, FileId};
use crate::syntax::is_reserved;

use super::chain::{
    HOLD_FLAGS, HeldChain, NtRequest, Owned, nt_create, open_handle, read_link, split_root,
};

#[cfg(test)]
#[path = "elevated_create_tests.rs"]
mod tests;

/// A new folder: the hold's access (a relative create needs no more of its parent handle), plus
/// delete for a rollback. Read and write are shared, delete never, so nobody renames it away.
const CREATE: NtRequest = NtRequest {
    access: FILE_TRAVERSE | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
    share: FILE_SHARE_READ | FILE_SHARE_WRITE,
    disposition: FILE_CREATE,
    options: FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
};

/// The step 3 re-resolve: attributes only and full sharing, so it opens beside every hold and
/// every created folder's delete access.
const CONFIRM: NtRequest = NtRequest {
    access: FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    disposition: FILE_OPEN,
    options: FILE_OPEN_REPARSE_POINT | FILE_OPEN_FOR_BACKUP_INTENT | FILE_SYNCHRONOUS_IO_NONALERT,
};

/// One folder this call created, held until the call returns.
struct Created {
    handle: Owned,
    id: FileId,
}

/// Creates `chain.chain.missing` below the held nearest folder, each with the descriptor `sddl`;
/// nothing missing is `Exists`. `chain` is `helper_chain`'s result and stays held by the caller.
///
/// 1. Every held ancestor, read again through its handle, must keep the walk's id and stay a
///    plain folder, and the nearest must be `args.parent`; else `Changed`, before any create.
/// 2. Each name is created relative to the previous handle. A name with `\`, `/`, `:` or NUL, one
///    ending in `.` or a space (so `.` and `..`), or a device name, is `Refused` before any create;
///    missing names that are not `args.root`'s last names are `Changed` before any create; a name
///    that exists is `Exists`; any other failure is `CreateFailed`.
/// 3. `args.root` is resolved again from the drive root: every id must equal the held and created
///    ones; else `Changed`.
///
/// On an error after a create, the created folders are deleted newest first through their own
/// handles; a folder that is no longer empty stays.
pub fn create_verified(args: &HelperArgs, chain: &HeldChain, sddl: &str) -> Result<(), HelperExit> {
    if chain.chain.missing.is_empty() {
        return Err(HelperExit::Exists);
    }
    if !chain.chain.missing.iter().all(|name| plain_name(name)) {
        return Err(HelperExit::Refused);
    }
    if !names_end(&args.root, &chain.chain.missing) {
        return Err(HelperExit::Changed);
    }
    let parent = held_parent(args, chain)?;
    let descriptor = security::descriptor(sddl).map_err(|_| HelperExit::CreateFailed)?;
    let mut created = Vec::new();
    let result = create_all(parent, &chain.chain.missing, &descriptor, &mut created)
        .and_then(|()| confirm(&args.root, &chain.chain, &created));
    if result.is_err() {
        remove_newest_first(created);
    }
    result
}

/// Whether `name` is one plain folder name: not empty, free of `\`, `/`, `:` and NUL, not ending
/// in `.` or a space, valid Unicode and no device name (`CON`, `COM1.txt`). The end rule refuses
/// `.` and `..`; it and the device rule refuse names an NT create keeps but a Win32 path misreads.
fn plain_name(name: &OsStr) -> bool {
    let units: Vec<u16> = name.encode_wide().collect();
    let banned = [0, u16::from(b'\\'), u16::from(b'/'), u16::from(b':')];
    let open_end = units
        .last()
        .is_some_and(|last| ![u16::from(b'.'), u16::from(b' ')].contains(last));
    let device = name.to_str().is_none_or(is_reserved);
    open_end && !device && !units.iter().any(|unit| banned.contains(unit))
}

/// Whether `missing` are the last names of `root`, so the creates build the folder step 3 checks.
fn names_end(root: &Path, missing: &[OsString]) -> bool {
    split_root(root).is_ok_and(|(_, names)| names.ends_with(missing))
}

/// Step 1 (IW42): checks every held ancestor through its own handle and returns the nearest one.
fn held_parent<'held>(
    args: &HelperArgs,
    chain: &'held HeldChain,
) -> Result<&'held Owned, HelperExit> {
    let handles = chain.handles();
    let existing = &chain.chain.existing;
    if handles.len() != existing.len() {
        return Err(HelperExit::Changed);
    }
    for (handle, link) in handles.iter().zip(existing) {
        if plain_id(handle).ok() != Some(link.id) {
            return Err(HelperExit::Changed);
        }
    }
    match (handles.last(), existing.last()) {
        (Some(handle), Some(link)) if link.id == args.parent => Ok(handle),
        _ => Err(HelperExit::Changed),
    }
}

/// Step 2: creates each name relative to the previous handle and keeps every new handle.
fn create_all(
    nearest: &Owned,
    names: &[OsString],
    descriptor: &Local,
    created: &mut Vec<Created>,
) -> Result<(), HelperExit> {
    for name in names {
        let parent = created.last().map_or(nearest, |folder| &folder.handle);
        let handle = nt_create(parent, name, &CREATE, Some(descriptor)).map_err(|error| {
            let exists = i32::try_from(ERROR_ALREADY_EXISTS)
                .is_ok_and(|code| error.raw_os_error() == Some(code));
            if exists {
                HelperExit::Exists
            } else {
                HelperExit::CreateFailed
            }
        })?;
        let Ok(id) = plain_id(&handle) else {
            // The newest folder goes first: it closes, and so is deleted, on return.
            delete_on_close(&handle);
            return Err(HelperExit::CreateFailed);
        };
        created.push(Created { handle, id });
    }
    Ok(())
}

/// Step 3: resolves `root` again, by fresh handles from the drive root, and compares every id.
fn confirm(root: &Path, chain: &Chain, created: &[Created]) -> Result<(), HelperExit> {
    let expected: Vec<FileId> = chain
        .existing
        .iter()
        .map(|link| link.id)
        .chain(created.iter().map(|folder| folder.id))
        .collect();
    match walk_ids(root) {
        Ok(found) if found == expected => Ok(()),
        _ => Err(HelperExit::Changed),
    }
}

/// The id of every folder on `path`, drive root first; a link or a file anywhere is an error.
fn walk_ids(path: &Path) -> std::io::Result<Vec<FileId>> {
    let (root, names) = split_root(path)?;
    let mut handle = open_handle(&root, CONFIRM.access, CONFIRM.share, HOLD_FLAGS)?;
    let mut ids = vec![plain_id(&handle)?];
    for name in names {
        handle = nt_create(&handle, &name, &CONFIRM, None)?;
        ids.push(plain_id(&handle)?);
    }
    Ok(ids)
}

/// A folder's id read through its handle; a reparse point or a file is an error.
fn plain_id(handle: &Owned) -> std::io::Result<FileId> {
    let (id, reparse, directory) = read_link(handle)?;
    if reparse || !directory {
        return Err(std::io::Error::other("not a plain folder"));
    }
    Ok(id)
}

/// Step 4: deletes the created folders newest first, each through its own handle on close.
fn remove_newest_first(mut created: Vec<Created>) {
    while let Some(folder) = created.pop() {
        delete_on_close(&folder.handle);
    }
}

/// Marks a held folder for deletion when its handle closes; a failure keeps the folder. A POSIX
/// delete frees the name on close even while another program (a scanner, the indexer) holds the
/// folder, so its parent can go next; a volume without it (FAT, older NTFS) gets the classic one.
fn delete_on_close(handle: &Owned) {
    let posix = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    if !set_info(handle, FileDispositionInfoEx, &posix) {
        set_info(
            handle,
            FileDispositionInfo,
            &FILE_DISPOSITION_INFO { DeleteFile: true },
        );
    }
}

/// Sets one information class on a held handle; whether the call succeeded.
fn set_info<T>(handle: &Owned, class: FILE_INFO_BY_HANDLE_CLASS, info: &T) -> bool {
    let Ok(size) = u32::try_from(size_of::<T>()) else {
        return false;
    };
    let info_ptr = std::ptr::from_ref(info).cast::<c_void>();
    // SAFETY: the handle is live with delete access and `info` is a readable `T` of `size` bytes.
    unsafe { SetFileInformationByHandle(handle.0, class, info_ptr, size) != 0 }
}
