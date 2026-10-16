use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_core::encoding::{TextEncoding, decode};

use super::{
    Log, Setup, editor_text, has_editor, hooks, press_only, redraw, text_of, text_window,
    window_on, window_with,
};
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};
use crate::window_view::viewer::{FileHooks, FileText, Kinds};

#[gpui_kit::test]
fn opening_shows_a_note_until_the_background_read_is_done(cx: &mut TestAppContext) {
    let (window, view, log) = window_on(cx, fake(), Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    assert!(viewer_open(cx, &view));
    assert_eq!(text_of(cx, window, "viewer-status"), "Opening…");
    assert!(log.heads().is_empty());
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(log.heads(), ["a.txt"]);
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "hello\n");
}

#[gpui_kit::test]
fn escape_while_opening_closes_and_the_late_result_is_dropped(cx: &mut TestAppContext) {
    let (window, view, log) = window_on(cx, fake(), Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    press_only(cx, window, "escape");
    assert!(!viewer_open(cx, &view));
    settle(cx, &view);
    redraw(cx, window);
    assert!(!viewer_open(cx, &view));
    assert_eq!(log.heads(), ["a.txt"]);
}

#[gpui_kit::test]
fn the_viewer_opens_read_only(cx: &mut TestAppContext) {
    let (window, view, log) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    type_in(cx, window, &view, "x");
    press(cx, window, &view, "ctrl-s");
    assert_eq!(editor_text(cx, &view), "hello\n");
    assert!(log.written().is_empty());
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn after_ctrl_e_unsaved_edits_make_escape_warn(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    assert_eq!(editor_text(cx, &view), "xhello\n");
    press(cx, window, &view, "escape");
    assert!(viewer_open(cx, &view));
    assert!(text_of(cx, window, "viewer-message").starts_with("Unsaved changes."));
}

#[gpui_kit::test]
fn saving_runs_in_the_background_and_editing_goes_on(cx: &mut TestAppContext) {
    let (window, view, log) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    press_only(cx, window, "ctrl-s");
    assert_eq!(text_of(cx, window, "viewer-message"), "Saving…");
    assert!(log.written().is_empty());
    type_in(cx, window, &view, "y");
    redraw(cx, window);
    assert_eq!(log.written(), ["xhello\n"]);
    assert_eq!(text_of(cx, window, "viewer-message"), "Saved.");
    assert_eq!(editor_text(cx, &view), "xyhello\n");
    press(cx, window, &view, "escape");
    assert!(viewer_open(cx, &view));
}

#[gpui_kit::test]
fn f3_on_a_utf16_file_opens_the_text_editor_with_the_decoded_text(cx: &mut TestAppContext) {
    let bytes: Vec<u8> = "\u{FEFF}héllo wide world\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let files = hooks(Setup::new(bytes.clone()), &Log::default());
    let read = Arc::new(move |_: &rusty_core::Location| {
        let decoded = decode(&bytes, TextEncoding::default().encoding);
        Ok(FileText {
            read_only: !decoded.lossless,
            encoding: decoded.encoding,
            ..FileText::plain(decoded.text)
        })
    });
    let files = FileHooks { read, ..files };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "héllo wide world\n");
}
