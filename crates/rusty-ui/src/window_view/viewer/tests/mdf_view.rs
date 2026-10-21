use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::structured::open_named;
use super::{Setup, has_editor, label, redraw, text_of, visible};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{press, settle};
use crate::window_view::viewer::mdf::tests::DEMO;

fn open_text(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, name, Setup::new(text.as_bytes().to_vec()));
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, &view);
        redraw(cx, window);
    }
    (window, view)
}

#[gpui_kit::test]
fn an_mdf_file_opens_as_a_chips_table_with_its_spaces_in_the_header(cx: &mut TestAppContext) {
    let (window, _) = open_text(cx, "engine.mdf", DEMO);
    assert_eq!(
        label(cx, window, ("mdf-row", 0)),
        "pflash0, ROM, 16.0 KB, linear, 0x80000000, 0x80001FFF, 0x00000000"
    );
    assert_eq!(
        text_of(cx, window, "mdf-spaces"),
        "Spaces: linear (id 1, 8-bit), data (id 2, 16-bit)"
    );
    assert_eq!(text_of(cx, window, "mdf-status"), "3 chips · 3 mappings");
}

#[gpui_kit::test]
fn a_malformed_line_shows_in_the_status_line(cx: &mut TestAppContext) {
    let text = format!("{DEMO}map 1 nope 4 10 0\n");
    let (window, _) = open_text(cx, "engine.mdf", &text);
    let status = text_of(cx, window, "mdf-status");
    assert!(status.starts_with("Line 10: "), "{status}");
    assert!(status.ends_with("3 chips · 3 mappings"), "{status}");
}

#[gpui_kit::test]
fn mdf_text_in_a_txt_file_opens_as_text(cx: &mut TestAppContext) {
    let (window, view) = open_text(cx, "notes.txt", DEMO);
    assert!(has_editor(cx, &view));
    assert!(!visible(cx, window, "mdf-status"));
}

#[gpui_kit::test]
fn ctrl_i_shows_the_selected_row_in_the_inspector(cx: &mut TestAppContext) {
    let (window, view) = open_text(cx, "engine.mdf", DEMO);
    press(cx, window, &view, "down");
    press(cx, window, &view, "ctrl-i");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    redraw(cx, window);
    assert!(visible(cx, window, "mdf-inspector"));
    let text = text_of(cx, window, "mdf-inspector-text");
    assert!(text.contains("Start in space: 0x80002000"), "{text}");
}
