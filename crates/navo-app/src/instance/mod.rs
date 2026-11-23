//! Single-instance lock and request channel: the first Navo per profile serves, later ones hand
//! their request over (Unix socket in a runtime folder, named pipe on Windows).

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
    #[error("the running Navo did not answer")]
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

/// Takes the single-instance lock for `profile_dir`; `portable_run` is the portable copy's
/// runtime folder (NV9a). `Ok(None)` means start without a lock; fails on channel errors.
pub(crate) fn acquire(
    profile_dir: &Path,
    portable_run: Option<&Path>,
) -> Result<Option<Acquired>, Error> {
    #[cfg(unix)]
    {
        if let Some(run) = portable_run {
            return acquire_portable(profile_dir, run);
        }
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
        acquire_with_runtime(profile_dir, runtime.as_deref(), &std::env::temp_dir())
    }
    #[cfg(windows)]
    {
        // Named pipes leave no file in any folder, so a portable copy needs no runtime folder.
        let _ = portable_run;
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

/// Takes the lock in the portable runtime folder `run`; an unsafe folder logs one warning and
/// starts without a lock, never falling back to a folder outside the portable copy.
#[cfg(unix)]
fn acquire_portable(profile_dir: &Path, run: &Path) -> Result<Option<Acquired>, Error> {
    match runtime::portable(run) {
        Ok(run) => acquire_in(profile_dir, &run).map(Some),
        Err(error) => {
            tracing::warn!(%error, "cannot use portable runtime folder; starting without instance lock");
            Ok(None)
        }
    }
}

pub(super) fn acquire_in(profile_dir: &Path, runtime: &Path) -> Result<Acquired, Error> {
    platform::acquire(&naming::profile_hash(profile_dir)?, runtime)
}

pub(crate) fn profile_hash(profile_dir: &Path) -> Result<String, Error> {
    naming::profile_hash(profile_dir)
}

#[cfg(unix)]
pub(crate) fn runtime_folder(slug: &str) -> std::path::PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
    runtime::folder(runtime.as_deref(), &std::env::temp_dir(), slug)
}

#[cfg(all(test, unix))]
mod portable_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod tests;
