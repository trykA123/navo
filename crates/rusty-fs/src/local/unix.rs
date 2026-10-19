use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use rusty_core::{FsErrorKind, ProviderPath};

use crate::Attributes;

const OWNER_WRITE: u32 = 0o200;

pub(super) fn os_path(path: &ProviderPath) -> Option<PathBuf> {
    path.to_bytes()
        .map(|bytes| PathBuf::from(OsString::from_vec(bytes)))
}

pub(super) fn entry_name(name: &OsStr) -> ProviderPath {
    ProviderPath::from_bytes(name.as_bytes())
}

pub(super) fn attributes(name: &OsStr, metadata: &Metadata) -> Attributes {
    Attributes {
        hidden: name.as_bytes().starts_with(b"."),
        system: false,
        readonly: metadata.permissions().mode() & OWNER_WRITE == 0,
        ..Attributes::default()
    }
}

pub(super) fn name_attributes(name: &OsStr) -> Attributes {
    Attributes {
        hidden: name.as_bytes().starts_with(b"."),
        ..Attributes::default()
    }
}

pub(super) fn error_kind(code: i32) -> FsErrorKind {
    FsErrorKind::from_errno(code)
}

pub(super) fn replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(target_os = "linux")]
pub(super) fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    use rustix::io::Errno;

    match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
        Err(Errno::INVAL | Errno::NOSYS) => link_then_unlink(from, to),
        other => other.map_err(io::Error::from),
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    link_then_unlink(from, to)
}

fn link_then_unlink(from: &Path, to: &Path) -> io::Result<()> {
    if fs::symlink_metadata(from)?.is_dir() {
        return match fs::symlink_metadata(to) {
            Ok(_) => Err(io::ErrorKind::AlreadyExists.into()),
            Err(_) => fs::rename(from, to),
        };
    }
    fs::hard_link(from, to)?;
    fs::remove_file(from)
}
