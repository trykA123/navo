#![cfg(windows)]

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use rusty_shell::{CaptureContent, Effect, FileClipboard, ShellCapture, Watcher, write_files};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

const WAIT: Duration = Duration::from_secs(5);

struct Owner(HWND);

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: this thread created the window and has closed the clipboard.
        unsafe { DestroyWindow(self.0) };
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "a watcher that cannot start must stop the test"
)]
fn watch() -> (Watcher, Receiver<ShellCapture>) {
    let (sender, received) = mpsc::channel();
    let watcher = Watcher::start(Box::new(move |capture| {
        let _ = sender.send(capture);
    }))
    .unwrap();
    (watcher, received)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_text(text: &str, markers: &[(&str, &[u8])]) -> Owner {
    let class = wide("STATIC");
    // SAFETY: STATIC is a system class and the invisible owner belongs to this thread.
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    assert!(!window.is_null());
    let owner = Owner(window);
    let mut opened = false;
    for _ in 0..50 {
        // SAFETY: window is a live window created on this thread.
        opened = unsafe { OpenClipboard(window) } != 0;
        if opened {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(opened);
    // SAFETY: this thread has opened the clipboard with an owner window.
    assert_ne!(unsafe { EmptyClipboard() }, 0);
    let units: Vec<u8> = wide(text).into_iter().flat_map(u16::to_le_bytes).collect();
    publish(u32::from(CF_UNICODETEXT), &units);
    for (name, value) in markers {
        let name = wide(name);
        // SAFETY: name is a live NUL-terminated UTF-16 string.
        let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
        assert_ne!(format, 0);
        publish(format, value);
    }
    // SAFETY: this thread opened the clipboard above.
    assert_ne!(unsafe { CloseClipboard() }, 0);
    owner
}

fn publish(format: u32, bytes: &[u8]) {
    // SAFETY: the allocation is movable and at least one byte long.
    let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
    assert!(!block.is_null());
    // SAFETY: block is a live allocation owned by this thread.
    let data = unsafe { GlobalLock(block) };
    assert!(!data.is_null());
    // SAFETY: the destination spans bytes.len() bytes and does not overlap the source.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast(), bytes.len()) };
    // SAFETY: this thread locked block once above.
    unsafe { GlobalUnlock(block) };
    // SAFETY: the clipboard is open and takes ownership of the movable unlocked block.
    assert!(!unsafe { SetClipboardData(format, block) }.is_null());
}

fn next_text(received: &Receiver<ShellCapture>) -> Option<(String, ShellCapture)> {
    let deadline = Instant::now() + WAIT;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let capture = received.recv_timeout(left).ok()?;
        if let CaptureContent::Text(text) = &capture.content {
            return Some((text.clone(), capture));
        }
    }
    None
}

#[test]
fn text_set_by_another_owner_arrives_once_with_its_exe() {
    let (_watcher, received) = watch();
    let _owner = set_text("watch test 🦀 one", &[]);
    let (text, capture) = next_text(&received).unwrap();
    assert_eq!(text, "watch test 🦀 one");
    let exe = capture.owner_exe.unwrap();
    assert!(exe.starts_with("clipboard_watch_windows"), "{exe}");
    let again = received.recv_timeout(Duration::from_millis(500));
    assert!(again.is_err(), "a second capture arrived");
}

#[test]
fn private_copies_are_skipped() {
    let (_watcher, received) = watch();
    let zero = 0u32.to_le_bytes();
    let private: [(&str, &[u8]); 3] = [
        ("CanIncludeInClipboardHistory", &zero),
        ("ExcludeClipboardContentFromMonitorProcessing", &[0]),
        ("Clipboard Viewer Ignore", &[0]),
    ];
    for marker in private {
        drop(set_text("private copy", &[marker]));
        std::thread::sleep(Duration::from_millis(200));
    }
    let allowed = 1u32.to_le_bytes();
    drop(set_text(
        "public after",
        &[("CanIncludeInClipboardHistory", &allowed)],
    ));
    let (text, _) = next_text(&received).unwrap();
    assert_eq!(text, "public after");
}

#[test]
fn a_cut_file_list_arrives_with_cut() {
    let (_watcher, received) = watch();
    let dir = tempfile::tempdir().unwrap();
    let paths = ["one.txt", "two 🦀.txt"].map(|name| dir.path().join(name));
    let files = FileClipboard {
        paths: paths.to_vec(),
        effect: Effect::Move,
    };
    write_files(&files).unwrap();
    let capture = received.recv_timeout(WAIT).unwrap();
    assert_eq!(
        capture.content,
        CaptureContent::Files {
            paths: paths.to_vec(),
            cut: true
        }
    );
    assert_ne!(capture.sequence, 0);
}

#[test]
fn drop_stops_the_thread_and_a_new_watcher_starts() {
    let (watcher, received) = watch();
    let started = Instant::now();
    drop(watcher);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        received.recv_timeout(Duration::from_secs(1)).err(),
        Some(RecvTimeoutError::Disconnected)
    );
    let (_watcher, received) = watch();
    let _owner = set_text("after restart", &[]);
    assert_eq!(next_text(&received).unwrap().0, "after restart");
}
