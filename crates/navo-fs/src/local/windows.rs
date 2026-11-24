use std::ffi::OsStr;
use std::fs::Metadata;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;

use navo_core::{FsErrorKind, ProviderPath};
use windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED;

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

/// Maps a Win32 error code to its kind; ERROR_OPERATION_ABORTED (995, a `CancelSynchronousIo`
/// from the I/O queue) is a cancel.
pub(super) fn error_kind(code: i32) -> FsErrorKind {
    match code.cast_unsigned() {
        ERROR_OPERATION_ABORTED => FsErrorKind::Cancelled,
        code => FsErrorKind::from_windows_code(code),
    }
}
