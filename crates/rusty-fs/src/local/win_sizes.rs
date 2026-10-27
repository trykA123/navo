#![allow(
    unsafe_code,
    reason = "std has no directory listing with allocation sizes and file ids"
)]

use std::io;
use std::mem::offset_of;
use std::ops::ControlFlow;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr;

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};
use windows_sys::Win32::Foundation::{
    ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO, FILE_ID_EXTD_DIR_INFO,
    FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FileIdBothDirectoryInfo, FileIdExtdDirectoryInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, OPEN_EXISTING,
};

use super::LocalFs;
use super::win_list::kind_of;
use crate::{CancelToken, EntryBatch, EntryKind, LinkKey, Provider, SizeEntry};

const BUFFER_BYTES: u32 = 64 * 1024;
const DOT: u16 = 0x2E;

struct Directory {
    handle: OwnedHandle,
    class: FILE_INFO_BY_HANDLE_CLASS,
    buffer: Vec<u64>,
    volume: u64,
    has_entries: bool,
}

pub(super) fn list_sizes(
    fs: &LocalFs,
    folder: &Location,
    sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    if let Some(directory) = open(folder, cancel)? {
        return directory.list(folder, sink, cancel);
    }
    fs.list(
        folder,
        &mut |batch: EntryBatch| {
            batch.entries.into_iter().try_for_each(|entry| {
                sink(SizeEntry {
                    name: entry.name,
                    kind: entry.kind,
                    size: entry.size,
                    on_disk: None,
                    link: None,
                })
            })
        },
        cancel,
    )
}

fn open(folder: &Location, cancel: &CancelToken) -> Result<Option<Directory>, FsError> {
    let path = super::os_path(folder)?;
    check_cancel(folder, cancel)?;
    let failure = |e: io::Error| super::open_failure(&e, &path, folder);
    let handle = open_handle(&path).map_err(failure)?;
    let volume = volume_serial(&handle).map_err(failure)?;
    let mut directory = Directory {
        handle,
        class: FileIdExtdDirectoryInfo,
        buffer: vec![0; BUFFER_BYTES as usize / size_of::<u64>()],
        volume,
        has_entries: false,
    };
    check_cancel(folder, cancel)?;
    let first = match directory.fill() {
        Err(error) if unsupported(&error) => {
            check_cancel(folder, cancel)?;
            directory.class = FileIdBothDirectoryInfo;
            directory.fill()
        }
        result => result,
    };
    match first {
        Err(error) if unsupported(&error) => Ok(None),
        result => {
            directory.has_entries = result.map_err(failure)?;
            Ok(Some(directory))
        }
    }
}

fn open_handle(path: &Path) -> io::Result<OwnedHandle> {
    let mut name: Vec<_> = path.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    name.push(0);
    // SAFETY: name is NUL-terminated; security and template pointers are null.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned a valid handle whose ownership transfers exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

fn volume_serial(handle: &OwnedHandle) -> io::Result<u64> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: handle remains open and info is a writable, correctly sized output struct.
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &raw mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(u64::from(info.dwVolumeSerialNumber))
}

fn unsupported(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().map(i32::cast_unsigned),
        Some(ERROR_INVALID_PARAMETER | ERROR_INVALID_LEVEL)
    )
}

impl Directory {
    fn fill(&mut self) -> io::Result<bool> {
        self.buffer.fill(0);
        // SAFETY: the live handle and 8-byte-aligned buffer cover the requested 64 KiB.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                self.handle.as_raw_handle(),
                self.class,
                self.buffer.as_mut_ptr().cast(),
                BUFFER_BYTES,
            )
        };
        if ok != 0 {
            return Ok(true);
        }
        let failure = io::Error::last_os_error();
        if failure.raw_os_error() == Some(ERROR_NO_MORE_FILES.cast_signed()) {
            return Ok(false);
        }
        Err(failure)
    }

    fn list(
        mut self,
        folder: &Location,
        sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        loop {
            check_cancel(folder, cancel)?;
            if !self.has_entries {
                return Ok(());
            }
            if self
                .send_buffer(sink)
                .map_err(|e| super::io_error(&e, folder))?
                .is_break()
            {
                return Ok(());
            }
            check_cancel(folder, cancel)?;
            self.has_entries = self.fill().map_err(|e| super::io_error(&e, folder))?;
        }
    }

    fn send_buffer(
        &self,
        sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
    ) -> io::Result<ControlFlow<()>> {
        // SAFETY: every byte of the allocated u64 buffer is initialized and lives for this borrow.
        let mut bytes = unsafe {
            std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), BUFFER_BYTES as usize)
        };
        loop {
            let next = u32::from_le_bytes(read(bytes, 0)?) as usize;
            let record = if next == 0 {
                bytes
            } else {
                if !next.is_multiple_of(8) {
                    return Err(invalid_record());
                }
                bytes.get(..next).ok_or_else(invalid_record)?
            };
            if let Some(entry) = self.entry(record)?
                && sink(entry).is_break()
            {
                return Ok(ControlFlow::Break(()));
            }
            if next == 0 {
                return Ok(ControlFlow::Continue(()));
            }
            bytes = bytes.get(next..).ok_or_else(invalid_record)?;
        }
    }

    fn entry(&self, bytes: &[u8]) -> io::Result<Option<SizeEntry>> {
        let name = self.file_name(bytes)?;
        if matches!(name.as_slice(), [DOT] | [DOT, DOT]) {
            return Ok(None);
        }
        let bits = u32::from_le_bytes(read(
            bytes,
            offset_of!(FILE_ID_EXTD_DIR_INFO, FileAttributes),
        )?);
        let kind = kind_of(bits, self.reparse_tag(bits, bytes)?);
        let file = kind == EntryKind::File;
        Ok(Some(SizeEntry {
            name: ProviderPath::from_wide(&name),
            kind,
            size: if file {
                read_size(bytes, offset_of!(FILE_ID_EXTD_DIR_INFO, EndOfFile))?
            } else {
                0
            },
            on_disk: if file {
                Some(read_size(
                    bytes,
                    offset_of!(FILE_ID_EXTD_DIR_INFO, AllocationSize),
                )?)
            } else {
                None
            },
            link: if file && bits & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                Some(LinkKey(self.file_id(bytes)?, self.volume))
            } else {
                None
            },
        }))
    }

    fn reparse_tag(&self, bits: u32, bytes: &[u8]) -> io::Result<u32> {
        if bits & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
            return Ok(0);
        }
        let offset = if self.class == FileIdExtdDirectoryInfo {
            offset_of!(FILE_ID_EXTD_DIR_INFO, ReparsePointTag)
        } else {
            offset_of!(FILE_ID_BOTH_DIR_INFO, EaSize)
        };
        Ok(u32::from_le_bytes(read(bytes, offset)?))
    }

    fn file_name(&self, bytes: &[u8]) -> io::Result<Vec<u16>> {
        let offset = if self.class == FileIdExtdDirectoryInfo {
            offset_of!(FILE_ID_EXTD_DIR_INFO, FileName)
        } else {
            offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)
        };
        let length = u32::from_le_bytes(read(
            bytes,
            offset_of!(FILE_ID_EXTD_DIR_INFO, FileNameLength),
        )?) as usize;
        if !length.is_multiple_of(2) {
            return Err(invalid_record());
        }
        let end = offset.checked_add(length).ok_or_else(invalid_record)?;
        Ok(bytes
            .get(offset..end)
            .ok_or_else(invalid_record)?
            .chunks_exact(2)
            .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
            .collect())
    }

    fn file_id(&self, bytes: &[u8]) -> io::Result<u128> {
        if self.class == FileIdExtdDirectoryInfo {
            Ok(u128::from_le_bytes(read(
                bytes,
                offset_of!(FILE_ID_EXTD_DIR_INFO, FileId),
            )?))
        } else {
            Ok(u128::from(u64::from_le_bytes(read(
                bytes,
                offset_of!(FILE_ID_BOTH_DIR_INFO, FileId),
            )?)))
        }
    }
}

fn read<const N: usize>(bytes: &[u8], offset: usize) -> io::Result<[u8; N]> {
    bytes
        .get(offset..offset + N)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(invalid_record)
}

fn read_size(bytes: &[u8], offset: usize) -> io::Result<u64> {
    u64::try_from(i64::from_le_bytes(read(bytes, offset)?)).map_err(|_| invalid_record())
}

fn invalid_record() -> io::Error {
    io::ErrorKind::InvalidData.into()
}

fn check_cancel(folder: &Location, cancel: &CancelToken) -> Result<(), FsError> {
    if cancel.is_cancelled() {
        return Err(super::error(FsErrorKind::Cancelled, folder));
    }
    Ok(())
}
