#![allow(
    unsafe_code,
    reason = "std has no safe wrapper for FindFirstFileExW with FIND_FIRST_EX_LARGE_FETCH"
)]

use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use rusty_core::ProviderPath;
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, FILETIME, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FIND_FIRST_EX_LARGE_FETCH, FindClose,
    FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, WIN32_FIND_DATAW,
};

use super::windows::file_attributes;
use crate::{EntryKind, FsEntry};

const NAME_SURROGATE: u32 = 0x2000_0000;
const UNIX_EPOCH_TICKS: i128 = 116_444_736_000_000_000;
const TICKS_PER_MS: i128 = 10_000;
const DOT: u16 = 0x2E;
const BACKSLASH: u16 = 0x5C;
const STAR: u16 = 0x2A;

struct FindHandle(HANDLE);

impl Drop for FindHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful FindFirstFileExW and is closed only here.
        unsafe { FindClose(self.0) };
    }
}

pub(super) struct NativeDir {
    handle: Option<FindHandle>,
    pending: Option<WIN32_FIND_DATAW>,
}

pub(super) fn open(path: &Path) -> io::Result<NativeDir> {
    let pattern = search_pattern(path)?;
    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: `pattern` is NUL-terminated and `data` is the WIN32_FIND_DATAW FindExInfoBasic fills.
    let handle = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FindExInfoBasic,
            (&raw mut data).cast(),
            FindExSearchNameMatch,
            ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if handle != INVALID_HANDLE_VALUE {
        return Ok(NativeDir {
            handle: Some(FindHandle(handle)),
            pending: Some(data),
        });
    }
    let failure = io::Error::last_os_error();
    let is_empty_folder = failure.raw_os_error() == Some(ERROR_FILE_NOT_FOUND.cast_signed())
        && fs::metadata(path).is_ok_and(|m| m.is_dir());
    if is_empty_folder {
        return Ok(NativeDir {
            handle: None,
            pending: None,
        });
    }
    Err(failure)
}

impl NativeDir {
    fn find_next(&mut self) -> Option<io::Result<WIN32_FIND_DATAW>> {
        let handle = self.handle.as_ref()?;
        let mut data = WIN32_FIND_DATAW::default();
        // SAFETY: `handle` is open and `data` is a live, writable WIN32_FIND_DATAW.
        let found = unsafe { FindNextFileW(handle.0, &raw mut data) };
        if found != 0 {
            return Some(Ok(data));
        }
        let failure = io::Error::last_os_error();
        self.handle = None;
        (failure.raw_os_error() != Some(ERROR_NO_MORE_FILES.cast_signed())).then_some(Err(failure))
    }
}

impl Iterator for NativeDir {
    type Item = io::Result<FsEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let data = match self.pending.take().map(Ok).or_else(|| self.find_next())? {
                Ok(data) => data,
                Err(failure) => return Some(Err(failure)),
            };
            if let Some(entry) = entry_of(&data) {
                return Some(Ok(entry));
            }
        }
    }
}

fn search_pattern(path: &Path) -> io::Result<Vec<u16>> {
    let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a folder path cannot contain NUL",
        ));
    }
    if units.last() != Some(&BACKSLASH) {
        units.push(BACKSLASH);
    }
    units.extend([STAR, 0]);
    Ok(units)
}

fn entry_of(data: &WIN32_FIND_DATAW) -> Option<FsEntry> {
    let names = &data.cFileName;
    let name = &names[..names
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(names.len())];
    if matches!(name, [DOT] | [DOT, DOT]) {
        return None;
    }
    let bits = data.dwFileAttributes;
    let kind = kind_of(bits, data.dwReserved0);
    let mut attributes = file_attributes(bits);
    attributes.link_to_folder = kind == EntryKind::Link && bits & FILE_ATTRIBUTE_DIRECTORY != 0;
    Some(FsEntry {
        name: ProviderPath::from_wide(name),
        kind,
        size: if kind == EntryKind::File {
            (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow)
        } else {
            0
        },
        modified_ms: modified_ms(data.ftLastWriteTime),
        attributes,
    })
}

pub(super) fn kind_of(bits: u32, reparse_tag: u32) -> EntryKind {
    if bits & FILE_ATTRIBUTE_REPARSE_POINT != 0 && reparse_tag & NAME_SURROGATE != 0 {
        EntryKind::Link
    } else if bits & FILE_ATTRIBUTE_DIRECTORY != 0 {
        EntryKind::Folder
    } else {
        EntryKind::File
    }
}

pub(super) fn modified_ms(time: FILETIME) -> Option<i64> {
    let ticks = (i128::from(time.dwHighDateTime) << 32) | i128::from(time.dwLowDateTime);
    i64::try_from((ticks - UNIX_EPOCH_TICKS) / TICKS_PER_MS).ok()
}
