#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use navo_shell::{
    CaptureContent, ClipboardError, ShellCapture, Watcher, clear_if_sequence, write_private_text,
    write_text,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

const WAIT: Duration = Duration::from_secs(5);
const MARKERS: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];

struct Open;

impl Open {
    fn new() -> Self {
        for _ in 0..50 {
            // SAFETY: a null owner opens the clipboard for reading on this thread.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Self;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the clipboard stayed busy");
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after a successful open on this thread.
        unsafe { CloseClipboard() };
    }
}

fn watch() -> (Watcher, Receiver<ShellCapture>) {
    let (sender, received) = mpsc::channel();
    let watcher = Watcher::start(Box::new(move |capture| {
        let _ = sender.send(capture);
    }))
    .unwrap();
    (watcher, received)
}

fn next_text(received: &Receiver<ShellCapture>) -> Option<String> {
    let deadline = Instant::now() + WAIT;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let CaptureContent::Text(text) = received.recv_timeout(left).ok()?.content {
            return Some(text);
        }
    }
    None
}

fn format(name: &str) -> u32 {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: wide is a live NUL-terminated UTF-16 string.
    let format = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
    assert_ne!(format, 0);
    format
}

fn read(format: u32) -> Option<Vec<u8>> {
    let _open = Open::new();
    // SAFETY: the query takes no pointers.
    if unsafe { IsClipboardFormatAvailable(format) } == 0 {
        return None;
    }
    // SAFETY: the clipboard is open on this thread until the bytes are copied.
    let handle = unsafe { GetClipboardData(format) };
    assert!(!handle.is_null());
    // SAFETY: the handle stays valid while the clipboard is open.
    let size = unsafe { GlobalSize(handle) };
    // SAFETY: as above.
    let data = unsafe { GlobalLock(handle) };
    assert!(!data.is_null());
    // SAFETY: the locked block spans size bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec();
    // SAFETY: locked once above.
    unsafe { GlobalUnlock(handle) };
    Some(bytes)
}

fn read_text() -> Option<String> {
    let bytes = read(u32::from(CF_UNICODETEXT))?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    Some(String::from_utf16(&units).unwrap())
}

#[test]
fn the_watcher_records_a_write_once() {
    let (_watcher, received) = watch();
    write_text("text test 🦀 once").unwrap();
    assert_eq!(next_text(&received).as_deref(), Some("text test 🦀 once"));
    assert!(
        received.recv_timeout(Duration::from_millis(500)).is_err(),
        "a second capture arrived"
    );
    assert_eq!(read_text().as_deref(), Some("text test 🦀 once"));
}

#[test]
fn the_watcher_ignores_a_private_write() {
    let (_watcher, received) = watch();
    write_private_text("text test private").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    write_text("text test public after").unwrap();
    assert_eq!(
        next_text(&received).as_deref(),
        Some("text test public after")
    );
}

#[test]
#[ignore = "needs an interactive desktop clipboard; run by the desktop tester"]
fn a_private_write_carries_the_three_markers() {
    let sequence = write_private_text("text test markers").unwrap();
    assert_ne!(sequence, 0);
    assert_eq!(read_text().as_deref(), Some("text test markers"));
    for name in MARKERS {
        assert!(read(format(name)).is_some(), "{name} is missing");
    }
    for name in &MARKERS[1..] {
        let value = read(format(name)).unwrap();
        assert_eq!(value.get(..4), Some(&[0u8; 4][..]), "{name}");
    }
    assert_eq!(clear_if_sequence(sequence), Ok(true));
}

#[test]
#[ignore = "needs an interactive desktop clipboard; run by the desktop tester"]
fn clear_if_sequence_empties_an_unchanged_clipboard() {
    let sequence = write_private_text("text test clear me").unwrap();
    assert_eq!(clear_if_sequence(sequence), Ok(true));
    assert_eq!(read_text(), None);
    for name in MARKERS {
        assert!(read(format(name)).is_none(), "{name} stayed");
    }
}

#[test]
fn clear_if_sequence_leaves_a_newer_copy_alone() {
    let sequence = write_private_text("text test old secret").unwrap();
    write_text("text test newer copy").unwrap();
    assert_eq!(clear_if_sequence(sequence), Ok(false));
    assert_eq!(read_text().as_deref(), Some("text test newer copy"));
}

#[test]
fn sequence_zero_never_clears() {
    write_text("text test zero").unwrap();
    assert_eq!(clear_if_sequence(0), Ok(false));
    assert_eq!(read_text().as_deref(), Some("text test zero"));
}

#[test]
fn an_embedded_nul_is_refused_and_keeps_the_clipboard() {
    write_text("text test before nul").unwrap();
    assert_eq!(write_text("a\0b"), Err(ClipboardError::Malformed));
    assert_eq!(
        write_private_text("secret\0tail"),
        Err(ClipboardError::Malformed)
    );
    assert_eq!(read_text().as_deref(), Some("text test before nul"));
}
