use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use rusty_core::{FsError, FsErrorKind, Location};

#[cfg(windows)]
mod windows;

pub fn make_link(target_dir: &Path, source: &Path) -> Result<PathBuf, FsError> {
    let target_dir = std::path::absolute(target_dir).map_err(|e| path_error(&e, target_dir))?;
    let source = std::path::absolute(source).map_err(|e| path_error(&e, source))?;
    let name = source
        .file_name()
        .ok_or_else(|| path_error(&io::ErrorKind::InvalidFilename.into(), &source))?;
    #[cfg(windows)]
    let shortcut = windows::Shortcut::new(&target_dir, &source, None)?;
    for number in 1_u64.. {
        let at = target_dir.join(link_name(name, number));
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&source, &at);
        #[cfg(windows)]
        let result = shortcut.publish(&at);
        match result {
            Ok(()) => return Ok(at),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(path_error(&error, &at)),
        }
    }
    Err(path_error(
        &io::ErrorKind::AlreadyExists.into(),
        &target_dir,
    ))
}

pub(super) fn link_at(target_of: &Location, at: &Location) -> Result<(), FsError> {
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(super::os_path(target_of)?, super::os_path(at)?);
    #[cfg(windows)]
    let result = {
        let source = plain_path(target_of)?;
        let place = super::os_path(at)?;
        let folder = place
            .parent()
            .ok_or_else(|| super::error(FsErrorKind::NameInvalid, at))?;
        windows::Shortcut::new(folder, &source, Some(at))?.publish(&place)
    };
    result.map_err(|e| super::io_error(&e, at))
}

pub(super) fn link_target(at: &Location) -> Result<Option<Location>, FsError> {
    let place = super::os_path(at)?;
    #[cfg(unix)]
    let found = std::fs::read_link(&place);
    #[cfg(windows)]
    let found = windows::read_target(&place);
    match found {
        Ok(target) => Ok(Location::local(super::platform::entry_name(target.as_os_str())).ok()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(super::io_error(&error, at)),
        Err(_) => Ok(None),
    }
}

pub(super) fn points_at(at: &Location, target_of: &Location) -> Result<bool, FsError> {
    let Some(found) = link_target(at)? else {
        return Ok(false);
    };
    #[cfg(unix)]
    let same = found == *target_of;
    #[cfg(windows)]
    let same = found == *target_of
        || matches!(
            (super::os_path(&found), super::os_path(target_of)),
            (Ok(found), Ok(wanted)) if windows::same_path(&found, &wanted)
        );
    Ok(same)
}

#[cfg(windows)]
fn plain_path(location: &Location) -> Result<PathBuf, FsError> {
    use std::os::windows::ffi::OsStringExt;
    location
        .is_local()
        .then(|| location.path().to_wide())
        .flatten()
        .map(|units| PathBuf::from(OsString::from_wide(&units)))
        .ok_or_else(|| super::error(FsErrorKind::NameInvalid, location))
}

fn link_name(name: &OsStr, number: u64) -> OsString {
    let mut name = name.to_os_string();
    #[cfg(windows)]
    name.push(" - Shortcut");
    if number > 1 {
        name.push(format!(" ({number})"));
    }
    #[cfg(windows)]
    name.push(".lnk");
    name
}

fn path_error(error: &io::Error, path: &Path) -> FsError {
    match Location::local(super::platform::entry_name(path.as_os_str())) {
        Ok(location) => super::io_error(error, &location),
        Err(_) => FsError::new(FsErrorKind::NameInvalid),
    }
}

#[cfg(test)]
mod tests;
