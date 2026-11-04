use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io;
use std::path::{Component, Path, PathBuf};

use super::{GuardError, PathRole};

pub(super) fn pin(path: &Path) -> io::Result<File> {
    File::open(path)
}

pub(super) fn final_path(
    _handle: &File,
    path: &Path,
    role: PathRole,
) -> Result<PathBuf, GuardError> {
    fs::canonicalize(path).map_err(|_| GuardError::Io(role))
}

pub(super) fn is_reparse(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(super) fn valid_component(component: Component<'_>) -> bool {
    !component.as_os_str().as_encoded_bytes().contains(&0)
}

pub(super) fn equal(left: &OsStr, right: &OsStr) -> bool {
    left == right
}

pub(super) fn sync_directory(handle: &File) -> io::Result<()> {
    handle.sync_all()
}
