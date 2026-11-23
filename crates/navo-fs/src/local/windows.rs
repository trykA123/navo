use std::ffi::OsStr;
use std::fs::Metadata;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;

use navo_core::{FsErrorKind, ProviderPath};

pub(super) use super::attribute_bits::file_attributes;
pub(super) use super::win_details::read as details;
pub(super) use super::win_replace::rename_new as rename_atomic;
pub(super) use super::win_replace::{rename_new, replace, replace_write_through};
pub(super) use super::win_volume::volume_serial;
use crate::Attributes;

pub(super) fn entry_name(name: &OsStr) -> ProviderPath {
    let units: Vec<u16> = name.encode_wide().collect();
    ProviderPath::from_wide(&units)
}

pub(super) fn attributes(_name: &OsStr, metadata: &Metadata) -> Attributes {
    file_attributes(metadata.file_attributes())
}

pub(super) fn name_attributes(_name: &OsStr) -> Attributes {
    Attributes::default()
}

pub(super) fn error_kind(code: i32) -> FsErrorKind {
    FsErrorKind::from_windows_code(code.cast_unsigned())
}
