use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;

use rusty_core::{FsErrorKind, ProviderPath};

use super::verbatim::verbatim;
pub(super) use super::win_details::read as details;
pub(super) use super::win_replace::rename_new as rename_atomic;
pub(super) use super::win_replace::{rename_new, replace};
pub(super) use super::win_volume::volume_serial;
use crate::Attributes;

const READONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;

pub(super) fn os_path(path: &ProviderPath) -> Option<PathBuf> {
    let units = path.to_wide()?;
    Some(PathBuf::from(OsString::from_wide(&verbatim(&units))))
}

pub(super) fn entry_name(name: &OsStr) -> ProviderPath {
    let units: Vec<u16> = name.encode_wide().collect();
    ProviderPath::from_wide(&units)
}

pub(super) fn attributes(_name: &OsStr, metadata: &Metadata) -> Attributes {
    file_attributes(metadata.file_attributes())
}

pub(super) fn file_attributes(bits: u32) -> Attributes {
    Attributes {
        hidden: bits & HIDDEN != 0,
        system: bits & SYSTEM != 0,
        readonly: bits & READONLY != 0,
        ..Attributes::default()
    }
}

pub(super) fn name_attributes(_name: &OsStr) -> Attributes {
    Attributes::default()
}

pub(super) fn error_kind(code: i32) -> FsErrorKind {
    FsErrorKind::from_windows_code(code.cast_unsigned())
}
