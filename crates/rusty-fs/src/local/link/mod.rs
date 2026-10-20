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
    let shortcut = windows::Shortcut::new(&target_dir, &source)?;
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
