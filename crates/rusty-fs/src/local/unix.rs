use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

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
