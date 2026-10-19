use std::fs::DirBuilder;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
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

pub(super) fn fallback(temp: &Path, uid: u32) -> io::Result<PathBuf> {
    let folder = temp.join(format!("ferrite-{uid}"));
    match DirBuilder::new().mode(0o700).create(&folder) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(&folder)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "instance runtime folder is not a private folder owned by the current user",
        ));
    }
    Ok(folder)
}
