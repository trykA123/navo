//! Tests for the hex inspector's hashes and strings: the pure scan over a byte source and the
//! drawer that shows its result, copies a hash and moves the cursor to a string.

use std::sync::atomic::AtomicBool;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

use super::{Setup, label, paged, paged_window, redraw};
use crate::window_view::tests::press;
use crate::window_view::viewer::paged::scan::{FoundString, SCAN_CHUNK, scan};

const FOO_CRC32: &str = "8c736521";
const FOO_SHA256: &str = "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae";
/// From the blake3 crate's own hex test.
const FOO_BLAKE3: &str = "04e0bb39f30b1a3feb89f536c93be15055482df748674b00d26e5a75777702e9";
const COUNTING_CRC32: &str = "a2912082";
const COUNTING_SHA256: &str = "c8f5d0341d54d951a71b136e6e2afcb14d11ed8489a7ae126a8fee0df6ecf193";

fn counting() -> Vec<u8> {
    (0..4096_u32).map(|at| (at % 256) as u8).collect()
}

fn strings_of(bytes: &[u8], chunk: usize, cap: usize) -> (Vec<(u64, String)>, bool) {
    let scanned = scan(bytes, chunk, cap, &AtomicBool::new(false))
        .expect("read")
        .expect("not cancelled");
    let found = scanned.strings.into_iter();
    let pairs = found.map(|FoundString { offset, text }| (offset, text));
    (pairs.collect(), scanned.capped)
}

#[test]
fn hex_inspector_scan_hashes_match_known_digests_for_any_chunk_size() {
    for chunk in [1, 2, 3, SCAN_CHUNK] {
        let scanned = scan(&b"foo"[..], chunk, 10, &AtomicBool::new(false))
            .expect("read")
            .expect("not cancelled");
        assert_eq!(scanned.digests.crc32, FOO_CRC32, "chunk {chunk}");
        assert_eq!(scanned.digests.sha256, FOO_SHA256, "chunk {chunk}");
        assert_eq!(scanned.digests.blake3, FOO_BLAKE3, "chunk {chunk}");
    }
    let bytes = counting();
    let scanned = scan(&bytes[..], 1000, 10, &AtomicBool::new(false))
        .expect("read")
        .expect("not cancelled");
    assert_eq!(scanned.digests.crc32, COUNTING_CRC32);
    assert_eq!(scanned.digests.sha256, COUNTING_SHA256);
}

#[test]
fn hex_inspector_scan_finds_runs_of_four_or_more_with_their_offsets() {
    let bytes = b"\x00abc\x01abcd\xffhello world\x00\x00xyz";
    for chunk in [1, 3, SCAN_CHUNK] {
        let (found, capped) = strings_of(bytes, chunk, 10);
        let expected = vec![(5, "abcd".to_owned()), (10, "hello world".to_owned())];
        assert_eq!(found, expected, "chunk {chunk}");
        assert!(!capped);
    }
    let (tail, _) = strings_of(b"\x00tail", 2, 10);
    assert_eq!(tail, vec![(1, "tail".to_owned())]);
}

#[test]
fn hex_inspector_scan_keeps_at_most_the_cap_and_says_so() {
    let bytes = b"aaaa\0bbbb\0cccc\0dddd\0eeee".to_vec();
    let (found, capped) = strings_of(&bytes, 4, 3);
    assert_eq!(found.len(), 3);
    assert_eq!(found[2], (10, "cccc".to_owned()));
    assert!(capped);
    let (all, capped) = strings_of(&bytes, 4, 5);
    assert_eq!(all.len(), 5);
    assert!(!capped);
}

#[test]
fn hex_inspector_scan_stops_when_cancelled() {
    let result = scan(&b"foo"[..], 1, 10, &AtomicBool::new(true)).expect("read");
    assert!(result.is_none());
}

/// Presses `key`, lets background work and fades finish, and draws a frame.
fn settle(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &gpui_kit::Entity<crate::window_view::NavoWindow>,
    key: &str,
) {
    press(cx, window, view, key);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    redraw(cx, window);
}

fn strings_title(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    cx.update_window(window, |_, window, _| {
        window
            .find("hex-strings-title")
            .label()
            .unwrap_or_default()
            .to_owned()
    })
    .expect("window")
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui_kit::test]
fn hex_inspector_drawer_shows_the_hashes_and_strings_after_settling(cx: &mut TestAppContext) {
    let bytes = counting();
    let blake = blake3::hash(&bytes).to_hex().to_string();
    let (window, view) = paged_window(cx, Setup::new(bytes));
    settle(cx, window, &view, "ctrl-i");
    assert_eq!(
        label(cx, window, ("hex-hash", 0)),
        format!("CRC32 {COUNTING_CRC32}")
    );
    assert_eq!(
        label(cx, window, ("hex-hash", 1)),
        format!("SHA-256 {COUNTING_SHA256}")
    );
    assert_eq!(
        label(cx, window, ("hex-hash", 2)),
        format!("BLAKE3 {blake}")
    );
    assert_eq!(strings_title(cx, window), "Strings · 16");
    let first = label(cx, window, ("hex-string-row", 0));
    assert!(first.starts_with("0x00000020  !\"#$"), "{first}");
}

#[gpui_kit::test]
fn hex_inspector_strings_and_hashes_work_by_keys_and_by_mouse(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(counting()));
    settle(cx, window, &view, "ctrl-i");
    let cursor = |cx: &mut TestAppContext| paged(cx, &view).read_with(cx, |paged, _| paged.cursor);
    settle(cx, window, &view, "tab");
    for _ in 0..4 {
        press(cx, window, &view, "down");
    }
    settle(cx, window, &view, "enter");
    assert_eq!(cursor(cx), 256 + 32);
    for _ in 0..4 {
        press(cx, window, &view, "up");
    }
    settle(cx, window, &view, "enter");
    assert_eq!(clipboard(cx).as_deref(), Some(COUNTING_CRC32));
    settle(cx, window, &view, "ctrl-home");
    assert_eq!(cursor(cx), 0);
    cx.update_window(window, |_, window, cx| {
        window.click(("hex-string-row", 1_usize), cx)
    })
    .expect("window");
    redraw(cx, window);
    assert_eq!(cursor(cx), 256 + 32);
    cx.update_window(window, |_, window, cx| {
        window.click(("hex-hash-copy", 1_usize), cx)
    })
    .expect("window");
    redraw(cx, window);
    assert_eq!(clipboard(cx).as_deref(), Some(COUNTING_SHA256));
    settle(cx, window, &view, "escape");
    assert!(!super::visible(cx, window, "hex-inspector"));
}
