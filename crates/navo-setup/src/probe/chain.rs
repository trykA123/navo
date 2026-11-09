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
use std::path::{Path, PathBuf};

use navo_core::brand;
use windows_sys::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, INVALID_HANDLE_VALUE, TRUE};
use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
    FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileAttributeTagInfo, FileCaseSensitiveInfo, FileIdInfo, GetFileInformationByHandleEx,
    OPEN_EXISTING,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemServices::FILE_CS_FLAG_CASE_SENSITIVE_DIR;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData,
    FOLDERID_LocalAppDataLow, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesCommon,
    FOLDERID_ProgramFilesX86, FOLDERID_Public, FOLDERID_RoamingAppData, FOLDERID_SkyDrive,
    FOLDERID_StartMenu, FOLDERID_Startup, FOLDERID_System, FOLDERID_UserProgramFiles,
    FOLDERID_Videos, FOLDERID_Windows, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows_sys::core::{GUID, PWSTR};

use crate::placement::{Chain, FileId, Link, NameEq, Protected, Refused};
use crate::syntax::Candidate;

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;

/// Win32 `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`.
const HRESULT_FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

/// Velopack's pack id, which is also the legacy root folder's name.
const NAVO_APP_ROOT: &str = "NavoApp";

/// Resolves the existing ancestors of a candidate and the names Setup would create.
pub fn resolve_chain(candidate: &Candidate) -> io::Result<Chain> {
    resolve_path(candidate.path())
}

/// Builds the IW8a refused folders, anchoring absent folders at their nearest existing parent.
pub fn refused() -> io::Result<Refused> {
    let mut never_inside = Vec::new();
    add_known(&mut never_inside, &FOLDERID_Windows, &[])?;
    add_known(&mut never_inside, &FOLDERID_System, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramFiles, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramFilesX86, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramFilesCommon, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramData, &[])?;
    add_known(&mut never_inside, &FOLDERID_LocalAppData, &[brand::NAME])?;
    add_known(&mut never_inside, &FOLDERID_RoamingAppData, &[brand::NAME])?;
    add_known(
        &mut never_inside,
        &FOLDERID_LocalAppData,
        &[brand::LEGACY_NAME],
    )?;
    add_known(
        &mut never_inside,
        &FOLDERID_RoamingAppData,
        &[brand::LEGACY_NAME],
    )?;
    add_known(&mut never_inside, &FOLDERID_SkyDrive, &[])?;
    add_known(&mut never_inside, &FOLDERID_Startup, &[])?;

    let mut never_equal = Vec::new();
    add_known(&mut never_equal, &FOLDERID_Profile, &[])?;
    add_known(&mut never_equal, &FOLDERID_Desktop, &[])?;
    add_known(&mut never_equal, &FOLDERID_Documents, &[])?;
    add_known(&mut never_equal, &FOLDERID_Downloads, &[])?;
    add_known(&mut never_equal, &FOLDERID_Pictures, &[])?;
    add_known(&mut never_equal, &FOLDERID_Music, &[])?;
    add_known(&mut never_equal, &FOLDERID_Videos, &[])?;
    add_known(&mut never_equal, &FOLDERID_LocalAppData, &[])?;
    add_known(&mut never_equal, &FOLDERID_RoamingAppData, &[])?;
    add_known(&mut never_equal, &FOLDERID_LocalAppDataLow, &[])?;
    add_known(&mut never_equal, &FOLDERID_UserProgramFiles, &[])?;
    add_known(&mut never_equal, &FOLDERID_StartMenu, &[])?;
    add_known(&mut never_equal, &FOLDERID_Public, &[])?;

    let mut fresh_only = Vec::new();
    add_known(&mut fresh_only, &FOLDERID_LocalAppData, &[NAVO_APP_ROOT])?;

    Ok(Refused {
        never_inside,
        never_equal,
        fresh_only,
    })
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

/// Resolves a path into its existing ancestors and missing names.
pub(crate) fn resolve_path(path: &Path) -> io::Result<Chain> {
    if !path.is_absolute() {
        return Err(io::Error::other("chain path is not absolute"));
    }
    let mut ancestors: Vec<&Path> = path.ancestors().collect();
    ancestors.reverse();

    let mut existing = Vec::new();
    let mut missing = Vec::new();
    let mut absent = false;
    for ancestor in ancestors {
        if absent {
            missing.push(missing_name(ancestor)?);
            continue;
        }
        match open_directory(ancestor) {
            Ok(handle) => {
                let (id, reparse, directory) = read_link(&handle)?;
                if !directory {
                    return Err(io::Error::other("chain ancestor is not a folder"));
                }
                existing.push(Link {
                    path: ancestor.to_path_buf(),
                    name: name(ancestor),
                    id,
                    reparse,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                absent = true;
                missing.push(missing_name(ancestor)?);
            }
            Err(error) => return Err(error),
        }
    }
    if existing.is_empty() {
        return Err(io::Error::other("chain has no existing ancestor"));
    }
    Ok(Chain { existing, missing })
}

/// Opens an existing folder with the flags IW7a names, without following a reparse point.
fn open_directory(path: &Path) -> io::Result<Owned> {
    let path = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and live for the call; no security attributes are used.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Owned(handle))
}

/// Reads a folder's identity, reparse flag and directory flag through its handle.
fn read_link(handle: &Owned) -> io::Result<(FileId, bool, bool)> {
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
fn case_sensitive(path: &Path) -> Option<bool> {
    let handle = open_directory(path).ok()?;
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
fn protected_from(path: &Path) -> io::Result<Protected> {
    let chain = resolve_path(path)?;
    let Some(anchor) = chain.existing.last() else {
        return Err(io::Error::other(
            "protected folder has no existing ancestor",
        ));
    };
    Ok(Protected {
        anchor: anchor.id,
        anchor_case_sensitive: case_sensitive(&anchor.path),
        tail: chain.missing,
    })
}

/// Adds one known folder to a refused set, skipping only an unresolvable known folder.
fn add_known(out: &mut Vec<Protected>, id: &GUID, suffix: &[&str]) -> io::Result<()> {
    let Some(base) = known_folder(id)? else {
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
    let mut raw: PWSTR = std::ptr::null_mut();
    // SAFETY: the GUID is a valid known-folder id and `raw` is a writable path pointer.
    let result = unsafe {
        SHGetKnownFolderPath(
            id,
            KF_FLAG_DEFAULT as u32,
            std::ptr::null_mut(),
            &raw mut raw,
        )
    };
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
    let owned = KnownPath(raw);
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
struct Owned(HANDLE);

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
