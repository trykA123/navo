mod naming;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
mod protocol;
mod request;
#[cfg(unix)]
mod runtime;
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

pub(crate) fn acquire(profile_dir: &Path) -> Result<Option<Acquired>, Error> {
    #[cfg(unix)]
    {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
        acquire_with_runtime(profile_dir, runtime.as_deref(), &std::env::temp_dir())
    }
    #[cfg(windows)]
    {
        acquire_in(profile_dir, Path::new("")).map(Some)
    }
}

#[cfg(unix)]
fn acquire_with_runtime(
    profile_dir: &Path,
    runtime: Option<&Path>,
    temp: &Path,
) -> Result<Option<Acquired>, Error> {
    let runtime = match runtime::resolve(runtime, temp) {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(%error, "cannot use instance runtime folder; starting without instance lock");
            return Ok(None);
        }
    };
    acquire_in(profile_dir, &runtime).map(Some)
}

pub(super) fn acquire_in(profile_dir: &Path, runtime: &Path) -> Result<Acquired, Error> {
    platform::acquire(&naming::profile_hash(profile_dir)?, runtime)
}

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod tests;
