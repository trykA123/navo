use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;

use rusty_core::{FsErrorKind, ProviderPath};

use crate::Attributes;

const READONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;
const BACKSLASH: u16 = b'\\' as u16;
const SLASH: u16 = b'/' as u16;
const VERBATIM: [u16; 4] = [BACKSLASH, BACKSLASH, b'?' as u16, BACKSLASH];
const VERBATIM_UNC: [u16; 8] = [
    BACKSLASH,
    BACKSLASH,
    b'?' as u16,
    BACKSLASH,
    b'U' as u16,
    b'N' as u16,
    b'C' as u16,
    BACKSLASH,
];

pub(super) fn os_path(path: &ProviderPath) -> Option<PathBuf> {
    let units = path.to_wide()?;
    Some(PathBuf::from(OsString::from_wide(&with_prefix(&units))))
}

fn is_separator(unit: u16) -> bool {
    unit == BACKSLASH || unit == SLASH
}

fn is_drive_letter(unit: u16) -> bool {
    u8::try_from(unit).is_ok_and(|byte| byte.is_ascii_alphabetic())
}

fn is_device_mark(unit: u16) -> bool {
    unit == u16::from(b'?') || unit == u16::from(b'.')
}

fn with_prefix(units: &[u16]) -> Vec<u16> {
    let (prefix, rest): (&[u16], &[u16]) = match units {
        [a, b, mark, c, ..]
            if is_separator(*a)
                && is_separator(*b)
                && is_device_mark(*mark)
                && is_separator(*c) =>
        {
            (&[], units)
        }
        [a, b, rest @ ..] if is_separator(*a) && is_separator(*b) => (&VERBATIM_UNC, rest),
        [letter, colon, separator, ..]
            if is_drive_letter(*letter)
                && *colon == u16::from(b':')
                && is_separator(*separator) =>
        {
            (&VERBATIM, units)
        }
        _ => (&[], units),
    };
    prefix
        .iter()
        .copied()
        .chain(rest.iter().map(|&u| if u == SLASH { BACKSLASH } else { u }))
        .collect()
}

pub(super) fn entry_name(name: &OsStr) -> ProviderPath {
    let units: Vec<u16> = name.encode_wide().collect();
    ProviderPath::from_wide(&units)
}

pub(super) fn attributes(_name: &OsStr, metadata: &Metadata) -> Attributes {
    let bits = metadata.file_attributes();
    Attributes {
        hidden: bits & HIDDEN != 0,
        system: bits & SYSTEM != 0,
        readonly: bits & READONLY != 0,
    }
}

pub(super) fn error_kind(code: i32) -> FsErrorKind {
    FsErrorKind::from_windows_code(code.cast_unsigned())
}
