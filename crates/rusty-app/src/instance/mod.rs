mod naming;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
mod protocol;
mod request;
mod server;

use std::path::Path;

pub(crate) use platform::{Client, Guard};
pub(crate) use request::Request;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("instance channel: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid instance request: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid instance request")]
    InvalidRequest,
    #[error("the running Ferrite did not answer")]
    Timeout,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    Ok,
    Error,
}

pub(crate) enum Acquired {
    First(Guard),
    Second(Client),
}

pub(crate) fn acquire(profile_dir: &Path) -> Result<Acquired, Error> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    acquire_in(profile_dir, &runtime)
}

pub(super) fn acquire_in(profile_dir: &Path, runtime: &Path) -> Result<Acquired, Error> {
    platform::acquire(&naming::profile_hash(profile_dir)?, runtime)
}

#[cfg(test)]
mod tests;
