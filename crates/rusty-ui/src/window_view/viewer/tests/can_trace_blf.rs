use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::structured::open_named;
use super::{Setup, has_editor, label, redraw, text_of, visible};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};

const CAN_MESSAGE: u32 = 1;
const LOG_CONTAINER: u32 = 10;

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn frame(raw_id: u32, timestamp: u64) -> Vec<u8> {
    let mut bytes = vec![0; 48];
    bytes[..4].copy_from_slice(b"LOBJ");
    put16(&mut bytes, 4, 32);
    put16(&mut bytes, 6, 1);
    put32(&mut bytes, 8, 48);
    put32(&mut bytes, 12, CAN_MESSAGE);
    put32(&mut bytes, 16, 2);
    bytes[24..32].copy_from_slice(&timestamp.to_le_bytes());
    put16(&mut bytes, 32, 1);
    bytes[35] = 2;
    put32(&mut bytes, 36, raw_id);
    bytes[40..42].copy_from_slice(&[0xAB, 0xCD]);
    bytes
}

fn blf() -> Vec<u8> {
    let inner: Vec<u8> = [(0x123, 1_000_000), (0x7DF, 2_000_000), (0x123, 3_000_000)]
        .into_iter()
        .flat_map(|(id, at)| frame(id, at))
        .collect();
    let mut container = vec![0; 32];
    container[..4].copy_from_slice(b"LOBJ");
    put16(&mut container, 4, 16);
    put16(&mut container, 6, 1);
    put32(&mut container, 8, (32 + inner.len()) as u32);
    put32(&mut container, 12, LOG_CONTAINER);
    put32(&mut container, 24, inner.len() as u32);
    container.extend(&inner);
    let mut bytes = vec![0; 144];
    bytes[..4].copy_from_slice(b"LOGG");
    put32(&mut bytes, 4, 144);
    put32(&mut bytes, 32, 3);
    for offset in [40, 56] {
        for (at, field) in [2026, 10, 2, 20, 12, 34, 56, 789].into_iter().enumerate() {
            put16(&mut bytes, offset + at * 2, field);
        }
    }
    bytes.extend(container);
    let size = bytes.len() as u64;
    bytes[16..24].copy_from_slice(&size.to_le_bytes());
    bytes
}

fn open(
    cx: &mut TestAppContext,
    name: &str,
    bytes: Vec<u8>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(bytes));
    for _ in 0..3 {
        settle(cx, &view);
        redraw(cx, window);
    }
    (window, view)
}

#[gpui_kit::test]
fn a_blf_opens_in_the_trace_view_whatever_its_extension(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "capture.dat", blf());
    assert_eq!(text_of(cx, window, "viewer-mode"), "Vector BLF");
    assert!(!has_editor(cx, &view));
}

#[gpui_kit::test]
fn blf_rows_show_decoded_frames(cx: &mut TestAppContext) {
    let (window, _) = open(cx, "bus.blf", blf());
    assert!(label(cx, window, ("trace-row", 0)).contains("123"));
    assert!(label(cx, window, ("trace-row", 0)).ends_with("AB CD"));
    assert!(label(cx, window, ("trace-row", 1)).contains("7DF"));
    assert!(text_of(cx, window, "trace-status").starts_with("3 frames"));
}

#[gpui_kit::test]
fn the_header_names_vector_blf(cx: &mut TestAppContext) {
    let (window, _) = open(cx, "bus.blf", blf());
    assert_eq!(text_of(cx, window, "viewer-mode"), "Vector BLF");
}

#[gpui_kit::test]
fn follow_is_unavailable_for_a_blf(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "bus.blf", blf());
    press(cx, window, &view, "alt-f");
    settle(cx, &view);
    redraw(cx, window);
    let note = text_of(cx, window, "trace-progress");
    assert_eq!(note, "Follow is not available for Vector BLF files");
    assert!(!text_of(cx, window, "trace-status").contains("following"));
}

#[gpui_kit::test]
fn a_text_file_starting_with_logging_is_not_a_trace(cx: &mut TestAppContext) {
    let (window, view) = open(cx, "notes.log", b"LOGGING started\nall fine\n".to_vec());
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "trace-status"));
    assert_eq!(text_of(cx, window, "viewer-mode"), "");
}
