//! Tests for the hex inspector: the pure decode table and the Ctrl+I drawer in hex mode.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::{Setup, label, paged, paged_window, redraw, visible};
use crate::window_view::tests::{press, viewer_open};
use crate::window_view::viewer::PagedKind;
use crate::window_view::viewer::paged::inspect::{Inspection, inspect};

const DASH: &str = "—";

/// A known buffer: f32 1.0 LE at 0, "é" in UTF-8 at 4, "A" in UTF-16 LE at 6, zeros 7–14.
const KNOWN: [u8; 16] = [
    0x00, 0x00, 0x80, 0x3F, 0xC3, 0xA9, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7F,
];

fn value<'a>(inspection: &'a Inspection, id: &str) -> &'a str {
    let row = inspection.rows.iter().find(|row| row.id == id);
    row.map(|row| row.value.as_str())
        .unwrap_or_else(|| panic!("no inspector row {id}"))
}

fn row_of(id: &str) -> usize {
    inspect(&[])
        .rows
        .iter()
        .position(|row| row.id == id)
        .expect("row")
}

#[test]
fn the_decode_table_reads_ints_floats_chars_and_times_at_the_cursor() {
    let at0 = inspect(&KNOWN);
    assert_eq!(value(&at0, "hex-inspector-f32-le"), "1.0");
    assert_eq!(value(&at0, "hex-inspector-u32-le"), "1065353216");
    assert_eq!(value(&at0, "hex-inspector-u32-be"), "32831");
    assert_eq!(value(&at0, "hex-inspector-u64-be"), "141011353944320");
    assert_eq!(value(&at0, "hex-inspector-i64-le"), "18482529535262720");
    let at2 = inspect(&KNOWN[2..]);
    assert_eq!(value(&at2, "hex-inspector-i8"), "-128");
    assert_eq!(value(&at2, "hex-inspector-u8"), "128");
    assert_eq!(value(&at2, "hex-inspector-u16-le"), "16256");
    assert_eq!(value(&at2, "hex-inspector-u16-be"), "32831");
    assert_eq!(value(&at2, "hex-inspector-i16-be"), "-32705");
    assert_eq!(value(&at2, "hex-inspector-bits"), "10000000");
    assert_eq!(
        value(&inspect(&KNOWN[4..]), "hex-inspector-utf8"),
        "é U+00E9"
    );
    let at6 = inspect(&KNOWN[6..]);
    assert_eq!(value(&at6, "hex-inspector-utf16-le"), "A U+0041");
    assert_eq!(value(&at6, "hex-inspector-utf16-be"), "䄀 U+4100");
    let at8 = inspect(&KNOWN[8..]);
    assert_eq!(
        value(&at8, "hex-inspector-unix-u32"),
        "1970-01-01T00:00:00Z"
    );
    let at7 = inspect(&KNOWN[7..15]);
    assert_eq!(
        value(&at7, "hex-inspector-unix-i64"),
        "1970-01-01T00:00:00Z"
    );
    assert_eq!(value(&at7, "hex-inspector-f64-be"), "0.0");
}

#[test]
fn a_cursor_on_the_last_byte_shows_a_dash_for_every_wider_type() {
    let last = inspect(&KNOWN[15..]);
    assert_eq!(value(&last, "hex-inspector-u8"), "127");
    assert_eq!(value(&last, "hex-inspector-utf8"), "U+007F");
    for id in [
        "hex-inspector-u16-le",
        "hex-inspector-i16-be",
        "hex-inspector-u32-be",
        "hex-inspector-i64-le",
        "hex-inspector-f32-le",
        "hex-inspector-f64-be",
        "hex-inspector-utf16-le",
        "hex-inspector-unix-u32",
        "hex-inspector-unix-i64",
    ] {
        assert_eq!(value(&last, id), DASH, "{id}");
    }
}

#[test]
fn a_unix_time_past_year_9999_shows_a_dash() {
    let far = inspect(&i64::MAX.to_le_bytes());
    assert_eq!(value(&far, "hex-inspector-unix-i64"), DASH);
    let before = inspect(&(-1_i64).to_le_bytes());
    assert_eq!(
        value(&before, "hex-inspector-unix-i64"),
        "1969-12-31T23:59:59Z"
    );
}

fn counting() -> Vec<u8> {
    (0..4096_u32).map(|at| (at % 256) as u8).collect()
}

/// Presses `key` and lets the drawer's fade finish.
fn press_settled(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    view: &gpui_kit::Entity<crate::window_view::NavoWindow>,
    key: &str,
) {
    press(cx, window, view, key);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    redraw(cx, window);
}

fn shown_u8(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> String {
    redraw(cx, window);
    label(
        cx,
        window,
        ("hex-inspector-row", row_of("hex-inspector-u8")),
    )
}

#[gpui_kit::test]
fn ctrl_i_in_hex_mode_opens_the_inspector_and_the_cursor_updates_it(cx: &mut TestAppContext) {
    let mut bytes = counting();
    bytes[0] = 0;
    let (window, view) = paged_window(cx, Setup::new(bytes));
    assert_eq!(super::kind(cx, &view), Some(PagedKind::Hex));
    assert!(!visible(cx, window, "hex-inspector"));
    press_settled(cx, window, &view, "ctrl-i");
    assert!(visible(cx, window, "hex-inspector"));
    assert!(shown_u8(cx, window).ends_with(" 0"));
    press(cx, window, &view, "right");
    assert!(shown_u8(cx, window).ends_with(" 1"));
    press(cx, window, &view, "down");
    assert!(shown_u8(cx, window).ends_with(" 17"));
    press(cx, window, &view, "left");
    assert!(shown_u8(cx, window).ends_with(" 16"));
    press(cx, window, &view, "ctrl-end");
    redraw(cx, window);
    let u16_row = label(
        cx,
        window,
        ("hex-inspector-row", row_of("hex-inspector-u16-le")),
    );
    assert!(u16_row.ends_with(DASH), "{u16_row}");
    press_settled(cx, window, &view, "escape");
    assert!(!visible(cx, window, "hex-inspector"));
    assert!(viewer_open(cx, &view));
}

#[gpui_kit::test]
fn a_click_on_a_byte_moves_the_cursor_there(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(counting()));
    press_settled(cx, window, &view, "ctrl-i");
    cx.update_window(window, |_, window, cx| {
        let size = window.find(("paged-hex", 1_usize)).bounds().size;
        let last_byte = gpui_kit::point(size.width - gpui_kit::px(1.), size.height / 2.);
        window.click_at(("paged-hex", 1_usize), last_byte, cx);
    })
    .expect("window");
    assert!(shown_u8(cx, window).ends_with(" 31"));
    let cursor = paged(cx, &view).read_with(cx, |paged, _| paged.cursor);
    assert_eq!(cursor, 31);
}
