use std::{ffi::OsString, path::PathBuf};

use navo_core::{FsError, FsErrorKind, Location};

pub mod identity;
#[cfg(any(windows, test))]
pub(crate) mod verbatim;

pub fn os_path(location: &Location) -> Result<PathBuf, FsError> {
    if !location.is_local() {
        return Err(error(FsErrorKind::NameInvalid, location));
    }
    platform_os_path(location.path()).ok_or_else(|| error(FsErrorKind::NameInvalid, location))
}

pub fn native_identity(path: &std::path::Path) -> std::io::Result<Option<crate::FileIdentity>> {
    identity::read(path)
}

pub fn error(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
    }
}

#[cfg(unix)]
fn platform_os_path(path: &navo_core::ProviderPath) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    path.to_bytes()
        .map(|bytes| PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(windows)]
fn platform_os_path(path: &navo_core::ProviderPath) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;

    use crate::native::verbatim::verbatim;
    let units = path.to_wide()?;
    Some(PathBuf::from(OsString::from_wide(&verbatim(&units))))
}
