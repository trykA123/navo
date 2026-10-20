use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{SessionFileError, usable};

const MAX_KEPT_COPIES: u32 = 1000;

pub(super) fn set_aside_unreadable(path: &Path) -> Result<(), SessionFileError> {
    match usable(path) {
        Err(
            SessionFileError::BadMagic
            | SessionFileError::BadChecksum
            | SessionFileError::Decode(_),
        ) => {
            let _ = fs::rename(path, suffixed(path, ".corrupt"));
            Ok(())
        }
        Err(SessionFileError::TooNew { found, .. }) => keep_newer(path, found),
        _ => Ok(()),
    }
}

pub(super) fn keep_newer_backup(backup: &Path) -> Result<(), SessionFileError> {
    match super::read_session_file(backup) {
        Err(SessionFileError::TooNew { found, .. }) => keep_newer(backup, found),
        _ => Ok(()),
    }
}

fn keep_newer(path: &Path, version: u16) -> Result<(), SessionFileError> {
    let kept = free_name(&suffixed(path, &format!(".v{version}")))?;
    fs::rename(path, kept)?;
    Ok(())
}

fn free_name(base: &Path) -> io::Result<PathBuf> {
    if fs::symlink_metadata(base).is_err() {
        return Ok(base.to_owned());
    }
    (2..MAX_KEPT_COPIES)
        .map(|n| suffixed(base, &format!(".{n}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("too many kept copies of {}", base.display()),
            )
        })
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
