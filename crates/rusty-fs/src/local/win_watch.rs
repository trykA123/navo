#![allow(
    unsafe_code,
    reason = "std has no safe wrapper for ReadDirectoryChangesW with overlapped I/O"
)]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use std::thread::JoinHandle;

use rusty_core::Location;
use windows_sys::Win32::Foundation::{
    ERROR_NOTIFY_ENUM_DIR, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, ResetEvent, SetEvent, WaitForMultipleObjects,
};

use super::{io_error, notify_info};
use crate::watch::{Depth, RawSender, Translate};

// Network shares reject change buffers above 64 KiB.
const BUFFER_BYTES: u32 = 64 * 1024;
const BUFFER_WORDS: usize = BUFFER_BYTES as usize / 4;
const FILTER: u32 = FILE_NOTIFY_CHANGE_FILE_NAME
    | FILE_NOTIFY_CHANGE_DIR_NAME
    | FILE_NOTIFY_CHANGE_SIZE
    | FILE_NOTIFY_CHANGE_LAST_WRITE
    | FILE_NOTIFY_CHANGE_ATTRIBUTES;

pub(super) struct FolderWatch {
    stop: Arc<OwnedHandle>,
    reader: Option<JoinHandle<()>>,
}

impl Drop for FolderWatch {
    fn drop(&mut self) {
        // SAFETY: `stop` is a live event handle this watch shares with its reader.
        unsafe { SetEvent(self.stop.as_raw_handle()) };
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub(super) fn start(
    path: &Path,
    folder: &Location,
    raw: RawSender,
    depth: Depth,
) -> io::Result<FolderWatch> {
    let stop = Arc::new(new_event()?);
    let (path, folder, stopper) = (path.to_owned(), folder.clone(), Arc::clone(&stop));
    let subtree = i32::from(depth == Depth::Subtree);
    let reader = std::thread::Builder::new()
        .name("folder-watch-rdcw".to_owned())
        .spawn(move || {
            if let Some(reader) = Reader::open(&path, folder, raw, stopper) {
                reader.run(subtree);
            }
        })?;
    Ok(FolderWatch {
        stop,
        reader: Some(reader),
    })
}

fn open_directory(path: &Path) -> io::Result<OwnedHandle> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a folder path cannot contain NUL",
        ));
    }
    wide.push(0);
    // SAFETY: `wide` is NUL-terminated; the other arguments are flags or null pointers.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            ptr::null_mut(),
        )
    };
    owned(handle)
}

fn new_event() -> io::Result<OwnedHandle> {
    // SAFETY: creates an unnamed manual-reset event with default security; pointers are null.
    let handle = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
    owned(handle)
}

fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the system just returned this open handle and nothing else owns it.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

enum Outcome {
    Filled(usize),
    Overflow,
    Stopped,
    Failed(io::Error),
}

struct Reader {
    directory: OwnedHandle,
    changed: OwnedHandle,
    stop: Arc<OwnedHandle>,
    raw: RawSender,
    folder: Location,
}

impl Reader {
    fn open(path: &Path, folder: Location, raw: RawSender, stop: Arc<OwnedHandle>) -> Option<Self> {
        let opened = open_directory(path)
            .and_then(|directory| new_event().map(|changed| (directory, changed)));
        match opened {
            Ok((directory, changed)) => Some(Self {
                directory,
                changed,
                stop,
                raw,
                folder,
            }),
            Err(failure) => {
                raw.failed(io_error(&failure, &folder));
                None
            }
        }
    }

    fn run(self, subtree: i32) {
        let mut buffer = vec![0u32; BUFFER_WORDS];
        loop {
            let delivered = match self.wait_for_changes(&mut buffer, subtree) {
                Outcome::Filled(0) | Outcome::Overflow => self.raw.overflow(),
                Outcome::Filled(bytes) => self.send(&buffer, bytes),
                Outcome::Stopped => return,
                Outcome::Failed(failure) => {
                    return self.raw.failed(io_error(&failure, &self.folder));
                }
            };
            if !delivered {
                return;
            }
        }
    }

    fn wait_for_changes(&self, buffer: &mut [u32], subtree: i32) -> Outcome {
        let directory = self.directory.as_raw_handle();
        let changed = self.changed.as_raw_handle();
        let mut overlapped = OVERLAPPED {
            hEvent: changed,
            ..OVERLAPPED::default()
        };
        let request = &raw mut overlapped;
        // SAFETY: `changed` is a live event owned by this reader.
        unsafe { ResetEvent(changed) };
        // SAFETY: `buffer` and `overlapped` outlive the request: every path below waits for it to
        // complete before returning, and nothing reads them while it is pending.
        let started = unsafe {
            ReadDirectoryChangesW(
                directory,
                buffer.as_mut_ptr().cast(),
                BUFFER_BYTES,
                subtree,
                FILTER,
                ptr::null_mut(),
                request,
                None,
            )
        };
        if started == 0 {
            return Outcome::Failed(io::Error::last_os_error());
        }
        self.raw.mark_live();
        let events = [changed, self.stop.as_raw_handle()];
        // SAFETY: both handles are live events owned by this reader or its watch.
        let woke = unsafe { WaitForMultipleObjects(2, events.as_ptr(), 0, INFINITE) };
        if woke != WAIT_OBJECT_0 {
            let failure = io::Error::last_os_error();
            // SAFETY: cancels only the request this reader issued on its own directory handle.
            unsafe { CancelIoEx(directory, request) };
            let _ = finish(directory, request, true);
            return if woke == WAIT_OBJECT_0 + 1 {
                Outcome::Stopped
            } else {
                Outcome::Failed(failure)
            };
        }
        match finish(directory, request, false) {
            Ok(bytes) => Outcome::Filled(bytes),
            Err(failure) if failure.raw_os_error() == Some(ERROR_NOTIFY_ENUM_DIR.cast_signed()) => {
                Outcome::Overflow
            }
            Err(failure) => Outcome::Failed(failure),
        }
    }

    fn send(&self, buffer: &[u32], filled: usize) -> bool {
        let mut changes = Vec::new();
        let mut translate = Translate::default();
        for (kind, name) in notify_info::parse(buffer, filled) {
            translate.feed(kind, name, &mut changes);
        }
        translate.flush(&mut changes);
        changes.into_iter().all(|change| self.raw.change(change))
    }
}

fn finish(directory: HANDLE, request: *const OVERLAPPED, wait: bool) -> io::Result<usize> {
    let mut bytes = 0u32;
    // SAFETY: `request` is the live OVERLAPPED of a request issued on `directory`.
    let done = unsafe { GetOverlappedResult(directory, request, &raw mut bytes, i32::from(wait)) };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes as usize)
}
