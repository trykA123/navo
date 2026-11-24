//! F3 editor folding and outline: files with a syntax highlighter get fold candidates from the
//! grammar, plain text gets none; Ctrl+Shift+O lists the file's symbols and jumps to one.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, MouseButton, TestAppContext};
use navo_fs::FakeFs;

use super::menu::{click_with, press_key, rows, run_item};
use super::{Setup, editor_of, redraw, window_on};
use crate::window_view::NavoWindow;
use crate::window_view::tests::press;

/// Opens `name` (holding `text`) with F3, switching to the Text view if a Rich view opened.
fn open_in_editor(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{name}"), 3);
    fs.add_folder("dest");
    let setup = Setup::new(text.as_bytes().to_vec());
    let (window, view, _) = window_on(cx, Arc::new(fs), setup);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    if editor_of(cx, &view).is_none() {
        press(cx, window, &view, "1");
    }
    redraw(cx, window);
    (window, view)
}

/// The lines (0-based) of `text` where the open editor offers a fold.
fn fold_lines(cx: &mut TestAppContext, name: &str, text: &str) -> Vec<usize> {
    let (_, view) = open_in_editor(cx, name, text);
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let lines = text.lines().count();
    editor.read_with(cx, |state, _| {
        (0..lines)
            .filter(|line| state.display_map().is_fold_candidate(*line))
            .collect()
    })
}

#[gpui_kit::test]
fn a_rust_function_offers_a_fold(cx: &mut TestAppContext) {
    let lines = fold_lines(cx, "lib.rs", "fn a() {\n    1\n}\n");
    assert_eq!(lines, vec![0]);
}

#[gpui_kit::test]
fn a_python_def_offers_a_fold(cx: &mut TestAppContext) {
    let lines = fold_lines(cx, "run.py", "def a():\n    x = 1\n    return x\n");
    assert!(lines.contains(&0), "{lines:?}");
}

#[gpui_kit::test]
fn a_multi_line_json_object_offers_a_fold(cx: &mut TestAppContext) {
    let lines = fold_lines(cx, "data.json", "{\n  \"a\": 1,\n  \"b\": 2\n}\n");
    assert!(lines.contains(&0), "{lines:?}");
}

#[gpui_kit::test]
fn markdown_headings_offer_a_fold(cx: &mut TestAppContext) {
    let text = "# One\n\nfirst text\nmore\n\n# Two\n\nsecond text\nmore\n";
    let lines = fold_lines(cx, "notes.md", text);
    assert!(!lines.is_empty(), "{lines:?}");
}

#[gpui_kit::test]
fn plain_text_offers_no_fold(cx: &mut TestAppContext) {
    let lines = fold_lines(cx, "notes.txt", "fn a() {\n    1\n}\n");
    assert!(lines.is_empty(), "{lines:?}");
}

const RUST: &str =
    "struct Point {\n    x: i32,\n}\n\nfn alpha() {}\n\nimpl Point {\n    fn beta(&self) {}\n}\n";

/// The (name, line) rows of the open outline popup, empty when none is open.
fn outline_rows(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<(String, String)> {
    view.read_with(cx, |view, cx| {
        let menu = view.viewer_menu_view()?;
        let items = menu.read(cx).menu().items.clone();
        let rows = items.into_iter().map(|item| {
            let line = item.accelerator.unwrap_or_default();
            (item.text, line)
        });
        Some(rows.collect())
    })
    .unwrap_or_default()
}

/// Opens `name` in the editor, presses Ctrl+Shift+O and returns the outline rows.
fn outline_of(cx: &mut TestAppContext, name: &str, text: &str) -> Vec<(String, String)> {
    let (window, view) = open_in_editor(cx, name, text);
    press(cx, window, &view, "ctrl-shift-o");
    outline_rows(cx, &view)
}

/// Turns `(name, line)` pairs into owned rows for comparing with `outline_rows`.
fn want(rows: &[(&str, u32)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(name, line)| ((*name).to_owned(), line.to_string()))
        .collect()
}

/// The open editor's cursor line (0-based).
fn cursor_line(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> u32 {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| state.cursor_position().line)
}

#[gpui_kit::test]
fn the_rust_outline_lists_items_with_their_lines(cx: &mut TestAppContext) {
    let rows = outline_of(cx, "lib.rs", RUST);
    let expected = [
        ("Point", 1),
        ("x", 2),
        ("alpha", 5),
        ("impl Point", 7),
        ("beta", 8),
    ];
    assert_eq!(rows, want(&expected));
}

#[gpui_kit::test]
fn the_python_outline_lists_classes_and_defs(cx: &mut TestAppContext) {
    let text = "class Shape:\n    def area(self):\n        return 0\n\ndef main():\n    pass\n";
    let rows = outline_of(cx, "run.py", text);
    assert_eq!(rows, want(&[("Shape", 1), ("area", 2), ("main", 5)]));
}

#[gpui_kit::test]
fn the_json_outline_lists_keys(cx: &mut TestAppContext) {
    let text = "{\n  \"name\": \"navo\",\n  \"deps\": {\n    \"gpui\": 1\n  }\n}\n";
    let rows = outline_of(cx, "data.json", text);
    assert_eq!(rows, want(&[("name", 2), ("deps", 3), ("gpui", 4)]));
}

#[gpui_kit::test]
fn the_markdown_outline_lists_headings(cx: &mut TestAppContext) {
    let rows = outline_of(cx, "notes.md", "# Intro\n\ntext\n\n## Usage\n\nmore\n");
    assert_eq!(rows, want(&[("Intro", 1), ("Usage", 5)]));
}

#[gpui_kit::test]
fn typing_narrows_the_outline_and_enter_jumps_to_the_symbol(cx: &mut TestAppContext) {
    let (window, view) = open_in_editor(cx, "lib.rs", RUST);
    press(cx, window, &view, "ctrl-shift-o");
    assert_eq!(rows(cx, window).len(), 5, "{:?}", rows(cx, window));
    cx.update_window(window, |_, window, cx| window.input("beta", cx))
        .expect("window");
    cx.run_until_parked();
    assert_eq!(rows(cx, window).len(), 1, "{:?}", rows(cx, window));
    press_key(cx, window, "enter");
    assert!(
        outline_rows(cx, &view).is_empty(),
        "Enter closes the outline"
    );
    assert_eq!(cursor_line(cx, &view), 7);
}

#[gpui_kit::test]
fn escape_closes_the_outline_and_a_click_jumps(cx: &mut TestAppContext) {
    let (window, view) = open_in_editor(cx, "lib.rs", RUST);
    press(cx, window, &view, "ctrl-shift-o");
    press_key(cx, window, "escape");
    assert!(outline_rows(cx, &view).is_empty(), "Esc closes the outline");
    assert_eq!(cursor_line(cx, &view), 0);
    press(cx, window, &view, "ctrl-shift-o");
    let at = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let mut root = window.within("context-menu");
            root.within(("menu-level", 0usize))
                .find(("menu-row", 2usize))
                .bounds()
                .center()
        })
        .expect("window");
    click_with(cx, window, (MouseButton::Left, at), true);
    click_with(cx, window, (MouseButton::Left, at), false);
    redraw(cx, window);
    assert!(
        outline_rows(cx, &view).is_empty(),
        "a click closes the outline"
    );
    assert_eq!(cursor_line(cx, &view), 4);
}

#[gpui_kit::test]
fn plain_text_has_no_outline(cx: &mut TestAppContext) {
    assert!(outline_of(cx, "notes.txt", "fn a() {}\n").is_empty());
}

/// Presses `key` and draws once, without running the background executor (no `run_until_parked`).
fn press_unparked(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
        window.render_frame(cx);
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_outline_opens_reading_and_fills_when_the_parse_lands(cx: &mut TestAppContext) {
    let (window, view) = open_in_editor(cx, "lib.rs", RUST);
    press_unparked(cx, window, "ctrl-shift-o");
    let reading = [("Reading outline\u{2026}".to_owned(), String::new())];
    assert_eq!(outline_rows(cx, &view), reading);
    cx.run_until_parked();
    assert_eq!(
        outline_rows(cx, &view).len(),
        5,
        "{:?}",
        outline_rows(cx, &view)
    );
}

#[gpui_kit::test]
fn the_view_menu_lists_outline_with_its_key_and_opens_it(cx: &mut TestAppContext) {
    let (window, view) = open_in_editor(cx, "lib.rs", RUST);
    press(cx, window, &view, "alt-v");
    let entry = ("Outline\u{2026}".to_owned(), "Ctrl+Shift+O".to_owned());
    let listed = outline_rows(cx, &view);
    assert!(listed.contains(&entry), "{listed:?}");
    run_item(cx, window, "Outline\u{2026}");
    cx.run_until_parked();
    assert_eq!(
        outline_rows(cx, &view).len(),
        5,
        "{:?}",
        outline_rows(cx, &view)
    );
}

#[gpui_kit::test]
fn a_result_for_an_older_text_revision_is_dropped(cx: &mut TestAppContext) {
    let (window, view) = open_in_editor(cx, "lib.rs", RUST);
    press_unparked(cx, window, "ctrl-shift-o");
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.land_stale_outline(window, cx));
    })
    .expect("window");
    let reading = [("Reading outline\u{2026}".to_owned(), String::new())];
    assert_eq!(
        outline_rows(cx, &view),
        reading,
        "the stale rows are dropped"
    );
    cx.run_until_parked();
    assert_eq!(
        outline_rows(cx, &view).len(),
        5,
        "{:?}",
        outline_rows(cx, &view)
    );
}
