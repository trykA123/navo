//! Windows chain probe: ancestors by file id and the IW8a refused folders.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "file ids, reparse tags and known folders require Win32 FFI"
)]

use std::ffi::{OsStr, OsString, c_void};
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_OPEN, FILE_OPEN_FOR_BACKUP_INTENT, FILE_OPEN_REPARSE_POINT as NT_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, E_FAIL, HANDLE, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError,
    TRUE, UNICODE_STRING,
};
use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
    FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_TRAVERSE, FileAttributeTagInfo, FileCaseSensitiveInfo, FileIdInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING, SYNCHRONIZE,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
use windows_sys::Win32::System::SystemServices::FILE_CS_FLAG_CASE_SENSITIVE_DIR;
use windows_sys::Win32::UI::Shell::{
    KF_FLAG_CREATE, KF_FLAG_DEFAULT, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath,
};
use windows_sys::core::{GUID, PWSTR};

use crate::payload::platform::security::Local;
use crate::placement::{Chain, FileId, Link, NameEq, Protected};
use crate::syntax::Candidate;

#[cfg(test)]
#[path = "chain_tests.rs"]
pub(crate) mod tests;

#[path = "refused.rs"]
mod refused;
pub use refused::refused;

/// The walk's access: traverse and attributes, never listing, so a folder that denies listing
/// still resolves.
///
/// `FILE_TRAVERSE` is `FILE_EXECUTE`'s bit, which share checks count as read access, so the
/// missing delete share still blocks a rename or delete; attribute access alone would not. A
/// relative `NtCreateFile` open needs no access on its `RootDirectory` handle: the file system
/// checks traverse against the parent's DACL, or skips it under the bypass-traverse privilege.
const HOLD_ACCESS: u32 = FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE;

/// The walk's share mode: read and write, never delete.
///
/// Denying write sharing would not stop `FSCTL_SET_REPARSE_POINT`: MS-FSA lets it run on an open
/// granted only `FILE_WRITE_ATTRIBUTES`, and such an open takes no part in share checks. So write
/// stays shared and `HeldChain::recheck` re-reads every tag before the helper answers instead.
const HOLD_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;

/// Opens a folder, never through a final reparse point.
pub(super) const HOLD_FLAGS: u32 = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT;

/// Win32 `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`.
const HRESULT_FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

/// Resolves the existing ancestors of a candidate and the names Setup would create.
pub fn resolve_chain(candidate: &Candidate) -> io::Result<Chain> {
    resolve_path(candidate.path())
}

/// Compares names with `CompareStringOrdinal` and case folding, as IW8a requires.
pub struct OrdinalNames;

impl NameEq for OrdinalNames {
    fn same(&self, a: &OsStr, b: &OsStr) -> bool {
        let (a, b) = (wide(a), wide(b));
        let (Ok(a_len), Ok(b_len)) = (i32::try_from(a.len()), i32::try_from(b.len())) else {
            return false;
        };
        // SAFETY: both buffers are live UTF-16 slices and the explicit lengths stay inside them.
        unsafe { CompareStringOrdinal(a.as_ptr(), a_len, b.as_ptr(), b_len, TRUE) == CSTR_EQUAL }
    }
}

/// Resolves a path into its existing ancestors and missing names; the handles close on return.
pub(crate) fn resolve_path(path: &Path) -> io::Result<Chain> {
    resolve_held(path).map(|held| held.chain)
}

/// A resolved chain whose existing folders stay open until it drops.
///
/// Every folder is held with traverse access and without delete sharing, so no ancestor can be
/// renamed or deleted while the chain is held, and what was checked is what stays in place.
pub struct HeldChain {
    /// The existing ancestors and missing names, read through the held handles.
    pub chain: Chain,
    /// One open handle per `chain.existing` link, drive root first.
    handles: Vec<Owned>,
}

impl HeldChain {
    /// The held handles, one per `chain.existing` link, drive root first.
    pub(super) fn handles(&self) -> &[Owned] {
        &self.handles
    }

    /// Re-reads every held folder's attributes and reparse tag; one that is now a link or no
    /// longer a folder refuses.
    ///
    /// The holds stop renames and deletes, not a reparse point set in place on an empty folder, so
    /// call this last, right before answering. Only an empty folder can take one (MS-FSA refuses a
    /// non-empty one), so the nearest folder is the exposed one; E5 creates through its handle.
    pub fn recheck(&self) -> io::Result<()> {
        for handle in &self.handles {
            let (_, reparse, directory) = read_link(handle)?;
            if reparse || !directory {
                return Err(io::Error::other("a held folder became a link"));
            }
        }
        Ok(())
    }
}

/// Resolves a path and keeps every existing ancestor open.
///
/// Opens the drive root, then each folder relative to its held parent without following links.
/// The walk ends at the first reparse point: the names below it count as missing and are never
/// resolved, so every caller that refuses a reparse link refuses the chain.
pub(crate) fn resolve_held(path: &Path) -> io::Result<HeldChain> {
    if !path.is_absolute() {
        return Err(io::Error::other("chain path is not absolute"));
    }
    let (root, names) = split_root(path)?;
    let root_handle = open_handle(&root, HOLD_ACCESS, HOLD_SHARE, HOLD_FLAGS)?;
    let mut existing = vec![link_of(&root_handle, root.clone(), name(&root))?];
    let mut handles = vec![root_handle];
    let mut current = root;
    let mut names = names.into_iter();
    while existing.last().is_some_and(|link| !link.reparse) {
        let Some(child) = names.next() else {
            break;
        };
        let parent = handles
            .last()
            .ok_or_else(|| io::Error::other("chain walk has no parent"))?;
        current.push(&child);
        match open_child(parent, &child) {
            Ok(handle) => {
                existing.push(link_of(&handle, current.clone(), child)?);
                handles.push(handle);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let missing = std::iter::once(child).chain(names).collect();
                return Ok(HeldChain {
                    chain: Chain { existing, missing },
                    handles,
                });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(HeldChain {
        chain: Chain {
            existing,
            missing: names.collect(),
        },
        handles,
    })
}

/// Splits an absolute path into its drive root and folder names; `..` and stream names refuse.
pub(super) fn split_root(path: &Path) -> io::Result<(PathBuf, Vec<OsString>)> {
    let mut root = PathBuf::new();
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir if names.is_empty() => {
                root.push(component.as_os_str());
            }
            Component::Normal(name) if !wide(name).contains(&u16::from(b':')) => {
                names.push(name.to_os_string());
            }
            Component::CurDir => {}
            _ => return Err(io::Error::other("chain path has an unsupported part")),
        }
    }
    Ok((root, names))
}

/// Reads one folder's link through its handle; a file is not a folder and refuses.
fn link_of(handle: &Owned, path: PathBuf, name: OsString) -> io::Result<Link> {
    let (id, reparse, directory) = read_link(handle)?;
    if !directory {
        return Err(io::Error::other("chain ancestor is not a folder"));
    }
    Ok(Link {
        path,
        name,
        id,
        reparse,
    })
}

/// Opens a protected anchor for its identity only: attribute access, full sharing, and `follow`
/// decides whether a final link is followed.
fn open_directory(path: &Path, follow: bool) -> io::Result<Owned> {
    let flags = if follow {
        FILE_FLAG_BACKUP_SEMANTICS
    } else {
        HOLD_FLAGS
    };
    let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
    open_handle(path, FILE_READ_ATTRIBUTES, share, flags)
}

/// Opens a folder by path with `CreateFileW`.
pub(super) fn open_handle(path: &Path, access: u32, share: u32, flags: u32) -> io::Result<Owned> {
    let path = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and live for the call; no security attributes are used.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            share,
            std::ptr::null(),
            OPEN_EXISTING,
            flags,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Owned(handle))
}

/// One relative `NtCreateFile` request: the access, share mode, disposition and options.
pub(super) struct NtRequest {
    pub(super) access: u32,
    pub(super) share: u32,
    pub(super) disposition: u32,
    pub(super) options: u32,
}

/// The walk's open: an existing folder, held, never through a final reparse point.
const HOLD_OPEN: NtRequest = NtRequest {
    access: HOLD_ACCESS,
    share: HOLD_SHARE,
    disposition: FILE_OPEN,
    options: NT_OPEN_REPARSE_POINT | FILE_OPEN_FOR_BACKUP_INTENT | FILE_SYNCHRONOUS_IO_NONALERT,
};

/// Opens one child by name relative to its held parent, without following a reparse point.
fn open_child(parent: &Owned, name: &OsStr) -> io::Result<Owned> {
    nt_create(parent, name, &HOLD_OPEN, None)
}

/// Opens or creates `name` relative to `parent` with `NtCreateFile`; the error is the status's
/// Win32 code. `descriptor` is a self-relative security descriptor for a new object, or none.
pub(super) fn nt_create(
    parent: &Owned,
    name: &OsStr,
    request: &NtRequest,
    descriptor: Option<&Local>,
) -> io::Result<Owned> {
    let mut units = wide(name);
    let length = u16::try_from(units.len().saturating_mul(2)).map_err(io::Error::other)?;
    let object_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: units.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).map_err(io::Error::other)?,
        RootDirectory: parent.0,
        ObjectName: &raw const object_name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: descriptor.map_or(std::ptr::null(), |local| local.0.cast()),
        ..OBJECT_ATTRIBUTES::default()
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: the parent handle, name buffer, descriptor and attribute structs stay live for the
    // call, and `handle` and `status_block` are writable; no allocation size or EA buffer is passed.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            request.access,
            &raw const attributes,
            &raw mut status_block,
            std::ptr::null(),
            0,
            request.share,
            request.disposition,
            request.options,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: a pure status-code conversion with no pointers.
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(
            i32::try_from(code).map_err(io::Error::other)?,
        ));
    }
    Ok(Owned(handle))
}

/// Reads a folder's identity, reparse flag and directory flag through its handle.
pub(super) fn read_link(handle: &Owned) -> io::Result<(FileId, bool, bool)> {
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

    let directory = tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    let reparse = tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || tag.ReparseTag != 0;
    let id = FileId {
        volume: info.VolumeSerialNumber,
        id: u128::from_le_bytes(info.FileId.Identifier),
    };
    Ok((id, reparse, directory))
}

/// Reads an anchor's case sensitivity; an error is `None`, which IW8a treats as unknown.
fn case_sensitive(handle: &Owned) -> Option<bool> {
    let mut info = FILE_CASE_SENSITIVE_INFO::default();
    let size = u32::try_from(size_of::<FILE_CASE_SENSITIVE_INFO>()).ok()?;
    let info_ptr = std::ptr::from_mut(&mut info).cast::<c_void>();
    // SAFETY: the handle is live and `info` is a writable struct of the size passed.
    let result =
        unsafe { GetFileInformationByHandleEx(handle.0, FileCaseSensitiveInfo, info_ptr, size) };
    if result == 0 {
        return None;
    }
    Some(info.Flags & FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0)
}

/// Turns one known folder plus a suffix into a `Protected` anchor and tail.
///
/// The anchor is the nearest existing folder opened through any link or mount point, so it is the
/// target's identity, which a candidate reaches by any path; an error other than not-found refuses.
/// A component that exists as a link whose target cannot be opened refuses too: only a name that
/// is truly absent falls back to its parent.
fn protected_from(path: &Path) -> io::Result<Protected> {
    let mut anchor = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match open_directory(&anchor, true) {
            Ok(handle) => {
                let (id, _, directory) = read_link(&handle)?;
                if !directory {
                    return Err(io::Error::other("protected folder is not a folder"));
                }
                tail.reverse();
                return Ok(Protected {
                    anchor: id,
                    anchor_case_sensitive: case_sensitive(&handle),
                    tail,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                absent_after(open_directory(&anchor, false))?;
                tail.push(missing_name(&anchor)?);
                anchor.pop();
            }
            Err(error) => return Err(error),
        }
    }
}

/// Judges the no-follow reopen of a name whose followed open was not found: only a not-found
/// reopen means the name is truly absent. A link that opens, or any other error such as access
/// denied, refuses, so a protected anchor never falls back to its lexical parent.
fn absent_after<T>(reopen: io::Result<T>) -> io::Result<()> {
    match reopen {
        Ok(_) => Err(io::Error::other(
            "a protected link's target cannot be resolved",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Adds a mandatory known folder: its configured path counts even when absent, and a failed
/// lookup refuses instead of dropping the anchor.
fn add_required(out: &mut Vec<Protected>, id: &GUID, suffix: &[&str]) -> io::Result<()> {
    add_required_from(out, known_folder_unverified(id), suffix)
}

/// Adds the result of a mandatory lookup; no path is an error.
fn add_required_from(
    out: &mut Vec<Protected>,
    base: io::Result<Option<PathBuf>>,
    suffix: &[&str],
) -> io::Result<()> {
    let base = base?.ok_or_else(|| io::Error::other("a mandatory known folder has no path"))?;
    add_base(out, Some(base), suffix)
}

/// Adds an explicitly optional known folder; only an `E_FAIL` or not-found lookup is skipped.
fn add_optional(out: &mut Vec<Protected>, id: &GUID, suffix: &[&str]) -> io::Result<()> {
    add_base(out, known_folder(id)?, suffix)
}

/// Adds an optional known folder whose path counts even when the folder does not exist yet.
fn add_optional_unverified(out: &mut Vec<Protected>, id: &GUID) -> io::Result<()> {
    add_base(out, known_folder_unverified(id)?, &[])
}

/// Adds a resolved known folder plus a suffix; an unresolved one is skipped.
fn add_base(out: &mut Vec<Protected>, base: Option<PathBuf>, suffix: &[&str]) -> io::Result<()> {
    let Some(base) = base else {
        return Ok(());
    };
    out.push(protected_from(&with_suffix(&base, suffix))?);
    Ok(())
}

/// Appends missing folder names to a known folder path.
fn with_suffix(base: &Path, suffix: &[&str]) -> PathBuf {
    let mut path = base.to_path_buf();
    for part in suffix {
        path.push(part);
    }
    path
}

/// Resolves one known folder; `None` is the allowed `E_FAIL` or file-not-found skip.
pub(crate) fn known_folder(id: &GUID) -> io::Result<Option<PathBuf>> {
    known_folder_with(id, KF_FLAG_DEFAULT)
}

/// Resolves a known folder's path without requiring the folder to exist and creates nothing.
pub(crate) fn known_folder_unverified(id: &GUID) -> io::Result<Option<PathBuf>> {
    known_folder_with(id, KF_FLAG_DONT_VERIFY)
}

/// Resolves a known folder and lets Windows create it with its normal ACL when it is missing.
pub(crate) fn create_known_folder(id: &GUID) -> io::Result<PathBuf> {
    known_folder_with(id, KF_FLAG_CREATE)?
        .ok_or_else(|| io::Error::other("the known folder cannot be created"))
}

/// Calls `SHGetKnownFolderPath` with `flags`; `None` is the allowed `E_FAIL` or not-found skip.
fn known_folder_with(id: &GUID, flags: i32) -> io::Result<Option<PathBuf>> {
    let mut raw: PWSTR = std::ptr::null_mut();
    // SAFETY: the GUID is a valid known-folder id and `raw` is a writable path pointer.
    let result = unsafe {
        SHGetKnownFolderPath(
            id,
            flags.cast_unsigned(),
            std::ptr::null_mut(),
            &raw mut raw,
        )
    };
    // Freed on every path: Microsoft documents the free even on failure, and a null free is a no-op.
    let owned = KnownPath(raw);
    if result < 0 {
        if result == E_FAIL || result == HRESULT_FILE_NOT_FOUND {
            return Ok(None);
        }
        return Err(io::Error::other(format!(
            "SHGetKnownFolderPath failed: 0x{result:08X}"
        )));
    }
    if raw.is_null() {
        return Err(io::Error::other("SHGetKnownFolderPath returned no path"));
    }
    Ok(Some(PathBuf::from(OsString::from_wide(owned.units()))))
}

/// A path allocated by `SHGetKnownFolderPath`, freed with `CoTaskMemFree` on drop.
struct KnownPath(PWSTR);

impl KnownPath {
    /// The NUL-terminated UTF-16 path without its terminator.
    fn units(&self) -> &[u16] {
        let mut len = 0;
        // SAFETY: the allocation is NUL-terminated and this guard keeps it alive.
        while unsafe { *self.0.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the first `len` units are initialized and inside the live allocation.
        unsafe { std::slice::from_raw_parts(self.0, len) }
    }
}

impl Drop for KnownPath {
    fn drop(&mut self) {
        // SAFETY: SHGetKnownFolderPath allocated this string with CoTaskMemAlloc; this guard owns it.
        unsafe { CoTaskMemFree(self.0.cast::<c_void>()) };
    }
}

/// A Win32 handle that closes on drop.
pub(super) struct Owned(pub(super) HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this guard owns a live handle that nothing else closes.
        unsafe { CloseHandle(self.0) };
    }
}

/// The file name of a chain path, or the path text for a drive root.
fn name(path: &Path) -> OsString {
    path.file_name().map_or_else(
        || path.as_os_str().to_os_string(),
        |name| name.to_os_string(),
    )
}

/// The name of a missing component; a path without one cannot be a folder chain.
fn missing_name(path: &Path) -> io::Result<OsString> {
    path.file_name()
        .map(|name| name.to_os_string())
        .ok_or_else(|| io::Error::other("missing chain component has no name"))
}

/// A NUL-terminated UTF-16 path for the Win32 directory calls.
fn wide_z(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A UTF-16 name without a terminator for `CompareStringOrdinal`.
fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().collect()
}
