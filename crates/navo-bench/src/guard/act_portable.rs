use std::ffi::OsStr;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::act::{Access, Identity, VerifiedItem};
use super::{GuardError, PathRole, write};

/// Opens the item itself, never a link target, with no data access.
pub(super) fn open_act(path: &Path) -> io::Result<File> {
    write::open_path_handle(path)
}

/// Holds a folder by path; unlike Windows, Unix cannot stop others from renaming it.
pub(super) fn hold_dir(path: &Path) -> io::Result<File> {
    write::open_path_handle(path)
}

/// Opens the folder `name` inside `parent`, relative to that handle, never through a link.
pub(super) fn hold_dir_at(parent: &File, name: &OsStr) -> io::Result<File> {
    write::open_path_handle(&child(parent, name)?)
}

/// Opens the item `name` inside `parent` itself, never a link target.
pub(super) fn open_act_at(parent: &File, name: &OsStr, _access: Access) -> io::Result<File> {
    write::open_path_handle(&child(parent, name)?)
}

/// Creates a new file `name` inside `parent`; an existing entry of any kind fails the call.
pub(super) fn create_file_at(parent: &File, name: &OsStr) -> io::Result<File> {
    write::open_new(&child(parent, name)?)
}

/// Creates a new folder `name` inside `parent`. Unix cannot make and open a folder in one call,
/// so the fake reopens it through the same parent handle at once.
pub(super) fn create_dir_at(parent: &File, name: &OsStr) -> io::Result<File> {
    let path = child(parent, name)?;
    write::make_dir(&path)?;
    write::open_path_handle(&path)
}

/// Unix cannot stop others from renaming a folder, so a held create is a plain create.
pub(super) fn create_held_dir_at(parent: &File, name: &OsStr) -> io::Result<File> {
    create_dir_at(parent, name)
}

/// `name` inside the folder `parent` is, reached through the handle, not a path to it.
fn child(parent: &File, name: &OsStr) -> io::Result<PathBuf> {
    if name.is_empty() || name.as_encoded_bytes().contains(&b'/') {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd())).join(name))
}

pub(super) fn pin_root(path: &Path) -> io::Result<File> {
    File::open(path)
}

pub(super) fn is_reparse(handle: &File, role: PathRole) -> Result<bool, GuardError> {
    let metadata = handle.metadata().map_err(|_| GuardError::Io(role))?;
    Ok(metadata.file_type().is_symlink())
}

pub(super) fn final_path(handle: &File, role: PathRole) -> Result<PathBuf, GuardError> {
    let link = PathBuf::from(format!("/proc/self/fd/{}", handle.as_raw_fd()));
    let path = fs::read_link(link).map_err(|_| GuardError::Io(role))?;
    if !path.is_absolute() {
        return Err(GuardError::Changed(role));
    }
    Ok(path)
}

pub(super) fn identity(handle: &File, role: PathRole) -> Result<Identity, GuardError> {
    let metadata = handle.metadata().map_err(|_| GuardError::Io(role))?;
    Ok(identity_of(&metadata))
}

/// Identity of whatever `path` names now, without following a final link.
pub(super) fn identity_at(path: &Path) -> io::Result<Option<Identity>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(identity_of(&metadata))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn delete(item: &VerifiedItem, role: PathRole) -> Result<(), GuardError> {
    write::remove_verified_file(item.path()).map_err(|_| GuardError::Io(role))
}

/// Removes a just-created item at the handle's real place, only while that place still names it.
pub(super) fn remove_stray(handle: &File, role: PathRole) -> Result<(), GuardError> {
    let path = final_path(handle, role)?;
    let metadata = handle.metadata().map_err(|_| GuardError::Io(role))?;
    let now = identity_at(&path).map_err(|_| GuardError::Io(role))?;
    if now != Some(identity_of(&metadata)) {
        return Err(GuardError::Changed(role));
    }
    let removed = if metadata.is_dir() {
        write::remove_verified_dir(&path)
    } else {
        write::remove_verified_file(&path)
    };
    removed.map_err(|_| GuardError::Io(role))
}

/// Removes the item a relative create made, only while `name` in `parent` still names it.
pub(super) fn remove_stray_at(
    parent: &File,
    name: &OsStr,
    handle: &File,
    role: PathRole,
) -> Result<(), GuardError> {
    let path = child(parent, name).map_err(|_| GuardError::Io(role))?;
    let metadata = handle.metadata().map_err(|_| GuardError::Io(role))?;
    let now = identity_at(&path).map_err(|_| GuardError::Io(role))?;
    if now != Some(identity_of(&metadata)) {
        return Err(GuardError::Changed(role));
    }
    let removed = if metadata.is_dir() {
        write::remove_verified_dir(&path)
    } else {
        write::remove_verified_file(&path)
    };
    removed.map_err(|_| GuardError::Io(role))
}

fn identity_of(metadata: &fs::Metadata) -> Identity {
    Identity {
        volume: metadata.dev(),
        file: u128::from(metadata.ino()),
    }
}
