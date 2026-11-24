//! F3 editor folding: files with a syntax highlighter get fold candidates from the
//! grammar; plain text gets none.

use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use navo_fs::FakeFs;

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
