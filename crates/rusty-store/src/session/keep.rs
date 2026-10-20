use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::{SessionFileError, usable};

const MAX_KEPT_COPIES: u32 = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FileRole {
    Main,
    Backup,
}

pub(super) fn set_aside_unreadable(path: &Path, role: FileRole) -> Result<(), SessionFileError> {
    match usable(path) {
        Ok(_) => Ok(()),
        Err(SessionFileError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(
            SessionFileError::BadMagic
            | SessionFileError::BadChecksum
            | SessionFileError::Decode(_),
        ) => {
            let moved = move_aside(path, &suffixed(path, ".corrupt"));
            match role {
                FileRole::Main => Ok(moved?),
                FileRole::Backup => Ok(()),
            }
        }
        Err(SessionFileError::TooNew { found, .. }) => {
            move_aside(path, &suffixed(path, &format!(".v{found}")))?;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn move_aside(path: &Path, base: &Path) -> io::Result<()> {
    let target = reserve_free_name(base)?;
    fs::rename(path, &target).inspect_err(|_| {
        let _ = fs::remove_file(&target);
    })
}

fn reserve_free_name(base: &Path) -> io::Result<PathBuf> {
    let candidates = std::iter::once(base.to_owned())
        .chain((2..MAX_KEPT_COPIES).map(|n| suffixed(base, &format!(".{n}"))));
    for candidate in candidates {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("too many kept copies of {}", base.display()),
    ))
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
