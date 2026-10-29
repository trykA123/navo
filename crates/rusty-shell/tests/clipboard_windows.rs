#![cfg(windows)]

use rusty_shell::{Effect, FileClipboard, clear_if_same, has_files, read_files, write_files};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, IsClipboardFormatAvailable, OpenClipboard,
};
use windows_sys::Win32::System::Ole::CF_HDROP;

struct Cleanup;

impl Drop for Cleanup {
    fn drop(&mut self) {
        for _ in 0..10 {
            // SAFETY: a null owner permits clearing, and every successful open is closed below.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                // SAFETY: this thread has the clipboard open and closes it before returning.
                unsafe {
                    EmptyClipboard();
                    CloseClipboard();
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[test]
fn clipboard_move_round_trips_and_clear_requires_matching_paths() {
    let dir = tempfile::tempdir().unwrap();
    let paths = ["one.txt", "two files 🦀.txt"].map(|name| dir.path().join(name));
    for path in &paths {
        std::fs::write(path, b"clipboard test").unwrap();
    }
    let _cleanup = Cleanup;
    let files = FileClipboard {
        paths: paths.to_vec(),
        effect: Effect::Move,
    };
    write_files(&files).unwrap();
    // SAFETY: querying format availability needs no pointers or open clipboard.
    let available = unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP)) };
    assert_ne!(available, 0);
    assert!(has_files());
    assert_eq!(read_files().unwrap(), Some(files.clone()));
    let different = FileClipboard {
        paths: vec![dir.path().join("different.txt")],
        effect: Effect::Move,
    };
    assert!(!clear_if_same(&different).unwrap());
    assert_eq!(read_files().unwrap(), Some(files.clone()));
    let same_paths = FileClipboard {
        effect: Effect::Copy,
        ..files.clone()
    };
    assert!(!clear_if_same(&same_paths).unwrap());
    write_files(&same_paths).unwrap();
    assert!(!clear_if_same(&files).unwrap());
    assert_eq!(read_files().unwrap(), Some(same_paths.clone()));
    replace_files(&same_paths);
    assert!(!clear_if_same(&same_paths).unwrap());
    assert_eq!(read_files().unwrap(), Some(same_paths.clone()));
    write_files(&same_paths).unwrap();
    assert!(clear_if_same(&same_paths).unwrap());
    assert_eq!(read_files().unwrap(), None);
    // SAFETY: querying format availability needs no pointers or open clipboard.
    let available = unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP)) };
    assert_eq!(available, 0);
    assert!(!has_files());
    assert!(!clear_if_same(&same_paths).unwrap());
}

fn replace_files(files: &FileClipboard) {
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    };
    use windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW;

    // SAFETY: STATIC is a system class and the invisible owner belongs to this thread.
    let owner = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::w!("STATIC"),
            windows::core::w!("external"),
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
    .expect("message-only owner window");
    // SAFETY: owner is a live window created on this thread.
    assert_ne!(unsafe { OpenClipboard(owner.0) }, 0);
    // SAFETY: this thread has opened the clipboard.
    assert_ne!(unsafe { EmptyClipboard() }, 0);
    let effect_name: Vec<_> = "Preferred DropEffect\0".encode_utf16().collect();
    // SAFETY: effect_name is a live NUL-terminated UTF-16 string.
    let effect = unsafe { RegisterClipboardFormatW(effect_name.as_ptr()) };
    assert_ne!(effect, 0);
    publish(effect, &files.effect.to_bytes());
    publish(
        u32::from(CF_HDROP),
        &rusty_shell::encode_drop_files(&files.paths),
    );
    // SAFETY: this thread opened the clipboard above.
    assert_ne!(unsafe { CloseClipboard() }, 0);
    // SAFETY: this thread created the owner, and the clipboard has been closed.
    unsafe { DestroyWindow(owner) }.expect("destroy the owner window");
}

fn publish(format: u32, bytes: &[u8]) {
    use windows_sys::Win32::System::DataExchange::SetClipboardData;
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };

    // SAFETY: the allocation is movable and sized for the nonempty format payload.
    let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
    assert!(!block.is_null());
    // SAFETY: block is a live allocation owned by this thread.
    let data = unsafe { GlobalLock(block) };
    assert!(!data.is_null());
    // SAFETY: the destination spans bytes.len() bytes and does not overlap the source.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast(), bytes.len()) };
    // SAFETY: this thread successfully locked block once above.
    unsafe { GlobalUnlock(block) };
    // SAFETY: the clipboard is open and takes ownership of the movable unlocked block.
    assert!(!unsafe { SetClipboardData(format, block) }.is_null());
}
