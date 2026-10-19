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
        ..files
    };
    assert!(clear_if_same(&same_paths).unwrap());
    assert_eq!(read_files().unwrap(), None);
    // SAFETY: querying format availability needs no pointers or open clipboard.
    let available = unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP)) };
    assert_eq!(available, 0);
    assert!(!has_files());
    assert!(!clear_if_same(&same_paths).unwrap());
}
