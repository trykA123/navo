use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr;
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::System::Ole::CF_HDROP;
use windows_sys::Win32::UI::Shell::SHAnsiToUnicode;

use super::{ClipboardError, Effect, FileClipboard, decode_drop_files, encode_drop_files};

struct Owner(HWND);

impl Owner {
    fn new() -> Result<Self, ClipboardError> {
        // SAFETY: STATIC is a system class; the invisible window belongs to this thread.
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::w!("STATIC"),
                windows::core::w!("Ferrite clipboard"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map_err(|error| ClipboardError::Os(format!("Creating the clipboard owner: {error}")))?;
        Ok(Self(window))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: this thread created the window, and the clipboard guard has already closed.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

struct Clipboard;

impl Clipboard {
    fn open(owner: windows_sys::Win32::Foundation::HWND) -> Result<Self, ClipboardError> {
        for attempt in 0..10 {
            // SAFETY: owner is null for reads or a live window owned by this thread.
            if unsafe { OpenClipboard(owner) } != 0 {
                return Ok(Self);
            }
            if attempt < 9 {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Err(ClipboardError::Busy)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after a successful open on the current thread.
        unsafe { CloseClipboard() };
    }
}

struct GlobalBlock(HGLOBAL);

impl GlobalBlock {
    fn new(bytes: &[u8]) -> Result<Self, ClipboardError> {
        // SAFETY: the requested allocation is nonzero and movable; zeroing covers allocator padding.
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) };
        if handle.is_null() {
            return Err(os_error("Allocating clipboard memory"));
        }
        let block = Self(handle);
        // SAFETY: the handle is a live allocation owned by block.
        let data = unsafe { GlobalLock(handle) };
        if data.is_null() {
            return Err(os_error("Locking clipboard memory"));
        }
        // SAFETY: data holds at least bytes.len() bytes and does not overlap the source.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast(), bytes.len()) };
        // SAFETY: this thread successfully locked this handle once.
        unsafe { GlobalUnlock(handle) };
        Ok(block)
    }

    fn publish(self, format: u32) -> Result<(), ClipboardError> {
        // SAFETY: the clipboard is open, and self owns an unlocked movable memory block.
        if unsafe { SetClipboardData(format, self.0) }.is_null() {
            return Err(os_error("Writing clipboard data"));
        }
        std::mem::forget(self);
        Ok(())
    }
}

impl Drop for GlobalBlock {
    fn drop(&mut self) {
        // SAFETY: this handle was never transferred to Windows and is no longer locked.
        unsafe { GlobalFree(self.0) };
    }
}

pub fn write_files(files: &FileClipboard) -> Result<(), ClipboardError> {
    if files.paths.iter().any(|path| {
        path.as_os_str().is_empty() || path.as_os_str().encode_wide().any(|unit| unit == 0)
    }) {
        return Err(ClipboardError::Malformed);
    }
    let paths = GlobalBlock::new(&encode_drop_files(&files.paths))?;
    let effect = GlobalBlock::new(&files.effect.to_bytes())?;
    let format = effect_format()?;
    let owner = Owner::new()?;
    let _clipboard = Clipboard::open(owner.0.0)?;
    empty()?;
    let published = effect
        .publish(format)
        .and_then(|()| paths.publish(u32::from(CF_HDROP)));
    if published.is_err() {
        let _ = empty();
    }
    published
}

pub fn read_files() -> Result<Option<FileClipboard>, ClipboardError> {
    let _clipboard = Clipboard::open(ptr::null_mut())?;
    read_open()
}

pub fn clear_if_same(files: &FileClipboard) -> Result<bool, ClipboardError> {
    let _clipboard = Clipboard::open(ptr::null_mut())?;
    let Some(current) = read_open()? else {
        return Ok(false);
    };
    if current.paths != files.paths {
        return Ok(false);
    }
    empty()?;
    Ok(true)
}

fn read_open() -> Result<Option<FileClipboard>, ClipboardError> {
    let Some(bytes) = read_data(u32::from(CF_HDROP))? else {
        return Ok(None);
    };
    let paths = decode_drop_files(&bytes)?;
    if paths.is_empty() {
        return Ok(None);
    }
    let effect_bytes = read_data(effect_format()?)?;
    let effect = Effect::from_bytes(effect_bytes.as_deref());
    Ok(Some(FileClipboard { paths, effect }))
}

fn read_data(format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
    // SAFETY: format is a registered identifier or CF_HDROP.
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        return Ok(None);
    }
    // SAFETY: the current thread holds the clipboard open until the bytes have been copied.
    let handle = unsafe { GetClipboardData(format) };
    if handle.is_null() {
        return Err(os_error("Reading clipboard data"));
    }
    // SAFETY: both supported formats store their data in an HGLOBAL returned by GetClipboardData.
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > isize::MAX as usize {
        return Err(ClipboardError::Malformed);
    }
    // SAFETY: the clipboard handle remains valid while the clipboard is open.
    let data = unsafe { GlobalLock(handle) };
    if data.is_null() {
        return Err(os_error("Locking clipboard data"));
    }
    // SAFETY: GlobalSize bounds the locked allocation, which remains locked during this copy.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec();
    // SAFETY: this thread successfully locked the clipboard handle once and never frees it.
    unsafe { GlobalUnlock(handle) };
    Ok(Some(bytes))
}

fn empty() -> Result<(), ClipboardError> {
    // SAFETY: the current thread holds the clipboard open.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(os_error("Emptying the clipboard"));
    }
    Ok(())
}

fn effect_format() -> Result<u32, ClipboardError> {
    let name: Vec<_> = "Preferred DropEffect\0".encode_utf16().collect();
    // SAFETY: name is a live NUL-terminated UTF-16 string.
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    if format == 0 {
        return Err(os_error("Registering Preferred DropEffect"));
    }
    Ok(format)
}

fn os_error(context: &str) -> ClipboardError {
    ClipboardError::Os(format!("{context}: {}", std::io::Error::last_os_error()))
}

pub(super) fn decode_ansi(bytes: &[u8]) -> Result<PathBuf, ClipboardError> {
    let source: Vec<_> = bytes.iter().copied().chain(std::iter::once(0)).collect();
    let capacity = i32::try_from(source.len()).map_err(|_| ClipboardError::Malformed)?;
    let mut wide = vec![0u16; source.len()];
    // SAFETY: source is NUL-terminated; wide has capacity units and never overlaps source.
    let count = unsafe { SHAnsiToUnicode(source.as_ptr(), wide.as_mut_ptr(), capacity) };
    let count = usize::try_from(count)
        .ok()
        .filter(|count| *count > 0 && *count <= wide.len())
        .ok_or(ClipboardError::Malformed)?;
    Ok(OsString::from_wide(&wide[..count - 1]).into())
}
