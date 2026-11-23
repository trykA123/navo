//! Unix instance runtime folder: `XDG_RUNTIME_DIR`, a private per-user temp folder, or the
//! portable copy's `<root>/data/run` (NV9a). Each is checked to be private before use.

use std::fs::DirBuilder;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

unsafe extern "C" {
    fn getuid() -> u32;
}

pub(super) fn resolve(runtime: Option<&Path>, temp: &Path) -> io::Result<PathBuf> {
    if let Some(runtime) = runtime {
        return Ok(runtime.to_path_buf());
    }
    // SAFETY: getuid takes no arguments and returns the calling process's real user id.
    fallback(temp, unsafe { getuid() })
}

pub(super) fn folder(runtime: Option<&Path>, temp: &Path, slug: &str) -> PathBuf {
    runtime.map_or_else(
        // SAFETY: getuid takes no arguments and returns the calling process's real user id.
        || temp.join(format!("{slug}-{}", unsafe { getuid() })),
        Path::to_path_buf,
    )
}

/// Longest portable runtime folder in bytes: `<run>/navo-<16 hex>.sock` must fit the 108-byte
/// `sun_path` with its NUL.
const MAX_PORTABLE_RUN_BYTES: usize = 80;

/// Creates the portable runtime folder `run` (mode 0700) and checks it is a real folder of the
/// current user; a copied folder with group or other bits is made private. Fails closed.
pub(super) fn portable(run: &Path) -> io::Result<PathBuf> {
    if run.as_os_str().len() > MAX_PORTABLE_RUN_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "portable runtime folder path is too long for a socket",
        ));
    }
    if let Some(parent) = run.parent() {
        std::fs::create_dir_all(parent)?;
    }
    create_private(run)?;
    // SAFETY: getuid takes no arguments and returns the calling process's real user id.
    let uid = unsafe { getuid() };
    if std::fs::symlink_metadata(run)?.mode() & 0o077 != 0 && owned_folder(run, uid)? {
        std::fs::set_permissions(run, std::fs::Permissions::from_mode(0o700))?;
    }
    check_private(run, uid)?;
    Ok(run.to_path_buf())
}

/// Creates `folder` with mode 0700; an existing entry is left for the caller to check.
fn create_private(folder: &Path) -> io::Result<()> {
    match DirBuilder::new().mode(0o700).create(folder) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
        _ => Ok(()),
    }
}

/// True when `folder` is a real folder (not a link) owned by `uid`.
fn owned_folder(folder: &Path, uid: u32) -> io::Result<bool> {
    let metadata = std::fs::symlink_metadata(folder)?;
    Ok(metadata.is_dir() && metadata.uid() == uid)
}

/// Fails with `PermissionDenied` unless `folder` is a real folder of `uid` with mode 0700 or tighter.
fn check_private(folder: &Path, uid: u32) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(folder)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "instance runtime folder is not a private folder owned by the current user",
        ));
    }
    Ok(())
}

/// Creates and checks the per-user fallback folder `<temp>/navo-<uid>`; fails closed.
pub(super) fn fallback(temp: &Path, uid: u32) -> io::Result<PathBuf> {
    let folder = temp.join(format!("{}-{uid}", navo_core::brand::SLUG));
    create_private(&folder)?;
    check_private(&folder, uid)?;
    Ok(folder)
}
