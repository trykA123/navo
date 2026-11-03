use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::System::Ole::CF_HDROP;
use windows_sys::Win32::UI::Shell::SHAnsiToUnicode;

use super::text_buffer::wipe;
use super::{ClipboardError, Effect, FileClipboard, decode_drop_files, encode_drop_files};

static WRITTEN_SEQUENCE: AtomicU32 = AtomicU32::new(0);

pub(super) struct Owner(pub(super) HWND);

impl Owner {
    pub(super) fn new() -> Result<Self, ClipboardError> {
        // SAFETY: STATIC is a system class; the invisible window belongs to this thread.
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::w!("STATIC"),
                windows::core::w!("Navo clipboard"),
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

pub(super) struct Clipboard;

impl Clipboard {
    fn open(owner: windows_sys::Win32::Foundation::HWND) -> Result<Self, ClipboardError> {
        Self::open_with(owner, 10, Duration::from_millis(10))
    }

    pub(super) fn open_with(
        owner: windows_sys::Win32::Foundation::HWND,
        attempts: u32,
        delay: Duration,
    ) -> Result<Self, ClipboardError> {
        for attempt in 0..attempts {
            // SAFETY: owner is null for reads or a live window owned by this thread.
            if unsafe { OpenClipboard(owner) } != 0 {
                return Ok(Self);
            }
            if attempt + 1 < attempts {
                std::thread::sleep(delay);
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

const LOCK_ATTEMPTS: u32 = 3;

/// Movable clipboard memory this process still owns. It stays locked from creation until it
/// is handed to the clipboard, and is zeroed before it is freed.
pub(super) struct GlobalBlock {
    handle: HGLOBAL,
    data: *mut u8,
    size: usize,
    locked: bool,
}

impl GlobalBlock {
    pub(super) fn new(bytes: &[u8]) -> Result<Self, ClipboardError> {
        // SAFETY: the requested allocation is nonzero and movable; zeroing covers allocator padding.
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) };
        if handle.is_null() {
            return Err(os_error("Allocating clipboard memory"));
        }
        // SAFETY: the handle is a live allocation owned by this function.
        let data = unsafe { GlobalLock(handle) }.cast::<u8>();
        if data.is_null() {
            let error = os_error("Locking clipboard memory");
            // SAFETY: the block is unlocked, holds nothing yet and was never shared.
            unsafe { GlobalFree(handle) };
            return Err(error);
        }
        // SAFETY: the handle is live and locked by this thread.
        let size = unsafe { GlobalSize(handle) }.max(bytes.len());
        // SAFETY: data holds at least bytes.len() bytes and does not overlap the source.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
        Ok(Self {
            handle,
            data,
            size,
            locked: true,
        })
    }

    pub(super) fn publish(mut self, format: u32) -> Result<(), ClipboardError> {
        self.hand_over(format)?;
        std::mem::forget(self);
        Ok(())
    }

    /// Unlocks and passes the block to the open clipboard; on failure it is locked again.
    fn hand_over(&mut self, format: u32) -> Result<(), ClipboardError> {
        self.unlock();
        // SAFETY: the clipboard is open, and self owns an unlocked movable memory block.
        if unsafe { SetClipboardData(format, self.handle) }.is_null() {
            let error = os_error("Writing clipboard data");
            self.relock();
            return Err(error);
        }
        Ok(())
    }

    fn unlock(&mut self) {
        if self.locked {
            // SAFETY: self holds exactly one lock on its live handle.
            unsafe { GlobalUnlock(self.handle) };
            self.locked = false;
        }
    }

    /// Locks again after a failed hand-over. If every attempt fails the block is freed
    /// unwiped: an unlocked movable block has no valid address to write through.
    fn relock(&mut self) {
        for _ in 0..LOCK_ATTEMPTS {
            // SAFETY: the handle is live, owned by self and not locked by it.
            let data = unsafe { GlobalLock(self.handle) }.cast::<u8>();
            if !data.is_null() {
                self.data = data;
                self.locked = true;
                return;
            }
        }
    }

    fn zero(&mut self) {
        if !self.locked {
            self.relock();
        }
        if !self.locked {
            return;
        }
        // SAFETY: the block is locked by self, so data spans size bytes at a valid address and
        // nothing else refers to them.
        let bytes = unsafe { std::slice::from_raw_parts_mut(self.data, self.size) };
        wipe(bytes);
    }
}

impl Drop for GlobalBlock {
    fn drop(&mut self) {
        self.zero();
        self.unlock();
        // SAFETY: the handle is unlocked, owned by self and was never handed to Windows.
        unsafe { GlobalFree(self.handle) };
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
    } else {
        WRITTEN_SEQUENCE.store(sequence(), Ordering::Relaxed);
    }
    published
}

#[must_use]
pub fn has_files() -> bool {
    // SAFETY: a format query needs no open clipboard and asks no owner to render data.
    unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP)) != 0 }
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
    if !super::model::matches_write(
        files,
        &current,
        Some(WRITTEN_SEQUENCE.load(Ordering::Relaxed)),
        Some(sequence()),
    ) {
        return Ok(false);
    }
    empty()?;
    Ok(true)
}

/// Whether the clipboard offers `format`; needs no open clipboard.
pub(super) fn format_available(format: u32) -> bool {
    // SAFETY: the query takes no pointers and does not render clipboard data.
    unsafe { IsClipboardFormatAvailable(format) != 0 }
}

pub(super) fn sequence() -> u32 {
    // SAFETY: the sequence query takes no pointers and does not render clipboard data.
    unsafe { GetClipboardSequenceNumber() }
}

pub(super) fn read_open() -> Result<Option<FileClipboard>, ClipboardError> {
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

pub(super) fn read_data(format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
    // SAFETY: format is a registered identifier or a standard format; the query takes no pointers.
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        return Ok(None);
    }
    // SAFETY: the current thread holds the clipboard open until the bytes have been copied.
    let handle = unsafe { GetClipboardData(format) };
    if handle.is_null() {
        return Err(os_error("Reading clipboard data"));
    }
    // SAFETY: every format read here (CF_HDROP, CF_UNICODETEXT, DWORD markers) is an HGLOBAL.
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

pub(super) fn empty() -> Result<(), ClipboardError> {
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

#[cfg(test)]
mod tests {
    use super::GlobalBlock;

    const CANARY: &[u8] = b"canary-secret";

    fn held(block: &GlobalBlock) -> &[u8] {
        // SAFETY: the block is locked and spans size bytes.
        unsafe { std::slice::from_raw_parts(block.data, block.size) }
    }

    #[test]
    fn a_failed_hand_over_leaves_the_block_locked_and_it_zeroes_through_that_lock() {
        let mut block = GlobalBlock::new(CANARY).unwrap();
        assert!(held(&block).starts_with(CANARY));
        assert!(block.hand_over(1).is_err(), "the clipboard is not open");
        assert!(block.locked);
        // SAFETY: hand_over re-locked the owned block and refreshed data after the failed call.
        unsafe { std::ptr::copy_nonoverlapping(CANARY.as_ptr(), block.data, CANARY.len()) };
        assert!(held(&block).starts_with(CANARY));
        block.zero();
        assert!(held(&block).iter().all(|byte| *byte == 0));
    }

    #[test]
    fn a_new_block_stays_locked_with_its_bytes_and_zeroed_padding() {
        let block = GlobalBlock::new(CANARY).unwrap();
        assert!(block.locked);
        assert!(block.size >= CANARY.len());
        let (text, padding) = held(&block).split_at(CANARY.len());
        assert_eq!(text, CANARY);
        assert!(padding.iter().all(|byte| *byte == 0));
    }
}
