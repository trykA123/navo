//! Chooses and verifies the private folder for user command list files.

#![allow(
    unsafe_code,
    reason = "the local app data folder and private directory checks need Win32 and geteuid"
)]

#[cfg(test)]
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::user_commands::error::StartError;

#[cfg(test)]
thread_local! {
    static LIST_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn with_list_root<T>(root: &Path, run: impl FnOnce() -> T) -> T {
    let previous = LIST_ROOT.with(|slot| slot.replace(Some(root.to_owned())));
    let result = run();
    LIST_ROOT.with(|slot| *slot.borrow_mut() = previous);
    result
}

/// The folder is checked and then created by path; a local attacker winning that window is out of scope.
pub(super) fn list_dir() -> Result<PathBuf, StartError> {
    #[cfg(test)]
    if let Some(root) = LIST_ROOT.with(|slot| slot.borrow().clone()) {
        return Ok(root);
    }
    system_list_dir()
}

#[cfg(windows)]
fn local_app_data() -> Result<PathBuf, StartError> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};

    let mut text = std::ptr::null_mut();
    // SAFETY: the API writes an owned, null-terminated UTF-16 allocation to text.
    let result = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_LocalAppData,
            0,
            std::ptr::null_mut(),
            &raw mut text,
        )
    };
    let local = if result >= 0 && !text.is_null() {
        let mut len = 0;
        // SAFETY: successful SHGetKnownFolderPath returns a null-terminated string.
        while unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: len spans the initialized string, excluding its terminator.
        let units = unsafe { std::slice::from_raw_parts(text, len) };
        PathBuf::from(OsString::from_wide(units))
    } else {
        // SAFETY: text is null or the allocation returned by SHGetKnownFolderPath.
        unsafe { CoTaskMemFree(text.cast()) };
        return Err(StartError::NoListRoot);
    };
    // SAFETY: text is null or the allocation returned by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(text.cast()) };
    Ok(local)
}

#[cfg(windows)]
fn system_list_dir() -> Result<PathBuf, StartError> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let local = local_app_data()?;
    let navo = local.join(navo_core::brand::NAME);
    let lists = navo.join("lists");
    for path in [&local, &navo, &lists] {
        match std::fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(StartError::ListFolder(error.kind())),
        }
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| StartError::ListFolder(error.kind()))?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(StartError::ListFolderNotPrivate);
        }
    }
    Ok(lists)
}

#[cfg(unix)]
fn system_list_dir() -> Result<PathBuf, StartError> {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        let leaf = runtime.join("navo-lists");
        create_list_dir(&leaf)?;
        check_unix_list(&leaf, Some(&runtime))?;
        return Ok(leaf);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(StartError::NoListRoot)?;
    let parent = home.join(".local/state/navo");
    create_list_dir(&parent)?;
    let leaf = parent.join("lists");
    create_list_dir(&leaf)?;
    check_unix_list(&leaf, Some(&home))?;
    Ok(leaf)
}

#[cfg(unix)]
fn create_list_dir(path: &Path) -> Result<(), StartError> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(StartError::ListFolder(error.kind())),
    }
}

#[cfg(unix)]
fn check_unix_list(leaf: &Path, allowed_parent: Option<&Path>) -> Result<(), StartError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata =
        std::fs::symlink_metadata(leaf).map_err(|error| StartError::ListFolder(error.kind()))?;
    // SAFETY: geteuid takes no arguments and returns the calling process's effective user ID.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir() || metadata.uid() != uid || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(StartError::ListFolderNotPrivate);
    }
    let parent = leaf.parent().ok_or(StartError::ListFolderNotPrivate)?;
    if allowed_parent == Some(parent) || special_parent(parent) {
        return Ok(());
    }
    let parent_metadata =
        std::fs::symlink_metadata(parent).map_err(|error| StartError::ListFolder(error.kind()))?;
    if !parent_metadata.is_dir()
        || parent_metadata.uid() != uid
        || parent_metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(StartError::ListFolderNotPrivate);
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn private_dir(dir: &Path) -> Result<(), StartError> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let metadata =
        std::fs::symlink_metadata(dir).map_err(|error| StartError::ListFolder(error.kind()))?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(StartError::ListFolderNotPrivate);
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn private_dir(dir: &Path) -> Result<(), StartError> {
    check_unix_list(dir, None)
}

#[cfg(unix)]
fn special_parent(parent: &Path) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    home.as_deref() == Some(parent) || runtime.as_deref() == Some(parent)
}
