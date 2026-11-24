//! Headless tests for Filter Lines in the editor's find bar: keep, remove, copy and bookmark
//! the lines that match the find pattern, from Ctrl+K commands and the "Filter lines" menu.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{Action, AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::search::editor_view;
use super::{editor_of, editor_text, text_of, window_with};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{press, settle, type_in};
use crate::window_view::viewer::keys::{
    BookmarkMatchingLines, CopyMatchingLines, KeepMatchingLines, RemoveMatchingLines,
};
use crate::window_view::viewer::{FileHooks, FileText, Head, Kinds};

/// Opens a window whose selected file reads as `file` and views it with F3; `edit` then
/// presses Ctrl+E.
fn open(
    cx: &mut TestAppContext,
    file: FileText,
    edit: bool,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let bytes = file.text.as_bytes().to_vec();
    let files = FileHooks {
        head: Arc::new(move |_| {
            Ok(Head {
                bytes: bytes.clone(),
                len: bytes.len() as u64,
            })
        }),
        read: Arc::new(move |_| Ok(file.clone())),
        ..FileHooks::none()
    };
    let (window, view) = window_with(
        cx,
        crate::window_view::tests::fake(),
        files,
        Kinds::builtin(),
    );
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    if edit {
        press(cx, window, &view, "ctrl-e");
    }
    (window, view)
}

/// Opens `text` in edit mode.
fn editing(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<NavoWindow>) {
    open(cx, FileText::plain(text), true)
}

/// Opens the find bar and types `pattern` into it.
fn find(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    pattern: &str,
) {
    press(cx, window, view, "ctrl-f");
    type_in(cx, window, view, pattern);
}

/// Runs a Filter Lines command as Ctrl+K would, then lets background work finish.
fn run(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    action: impl Action,
) {
    cx.dispatch_action(window, action);
    settle(cx, view);
    cx.run_until_parked();
}

fn bookmarks(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<u64> {
    let editor = editor_view(cx, view);
    editor.read_with(cx, |editor, _| {
        editor.marking_state().bookmarks.iter().collect()
    })
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui_kit::test]
fn keep_matching_lines_keeps_only_those_lines(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "apple\nbanana\napricot\n");
    find(cx, window, &view, "ap");
    run(cx, window, &view, KeepMatchingLines);
    assert_eq!(editor_text(cx, &view), "apple\napricot\n");
}

#[gpui_kit::test]
fn one_undo_after_remove_matching_lines_restores_the_text(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "apple\nbanana\napricot\n");
    find(cx, window, &view, "ap");
    run(cx, window, &view, RemoveMatchingLines);
    assert_eq!(editor_text(cx, &view), "banana\n");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-z");
    assert_eq!(editor_text(cx, &view), "apple\nbanana\napricot\n");
}

#[gpui_kit::test]
fn a_last_line_without_a_newline_stays_without_one(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "a1\nb\na2");
    find(cx, window, &view, "2");
    run(cx, window, &view, RemoveMatchingLines);
    assert_eq!(editor_text(cx, &view), "a1\nb");
}

#[gpui_kit::test]
fn copy_matching_lines_joins_them_with_the_files_crlf(cx: &mut TestAppContext) {
    let file = FileText {
        crlf: true,
        ..FileText::plain("one\ntwo\nthree\n")
    };
    let (window, view) = open(cx, file, false);
    find(cx, window, &view, "o");
    run(cx, window, &view, CopyMatchingLines);
    assert_eq!(clipboard(cx).as_deref(), Some("one\r\ntwo"));
    assert_eq!(text_of(cx, window, "viewer-message"), "Copied 2 lines.");
    assert_eq!(editor_text(cx, &view), "one\ntwo\nthree\n");
}

#[gpui_kit::test]
fn bookmark_all_marks_every_matching_line_and_keeps_old_marks(cx: &mut TestAppContext) {
    let (window, view) = open(cx, FileText::plain("x\nab\ny\nab\n"), false);
    let editor = editor_of(cx, &view).expect("the text editor is open");
    editor.update(cx, |state, cx| state.set_selected_range(2..2, cx));
    press(cx, window, &view, "ctrl-f2");
    assert_eq!(bookmarks(cx, &view), [2]);
    find(cx, window, &view, "ab");
    run(cx, window, &view, BookmarkMatchingLines);
    assert_eq!(bookmarks(cx, &view), [2, 7]);
}

#[gpui_kit::test]
fn a_regex_pattern_and_match_case_are_respected(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "Error 1\nerror 2\nwarn\n");
    press(cx, window, &view, "ctrl-f");
    press(cx, window, &view, "alt-r");
    press(cx, window, &view, "alt-c");
    type_in(cx, window, &view, r"^E\w+");
    run(cx, window, &view, KeepMatchingLines);
    assert_eq!(editor_text(cx, &view), "Error 1\n");
}

#[gpui_kit::test]
fn an_empty_pattern_changes_nothing(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "a\nb\n");
    press(cx, window, &view, "ctrl-f");
    run(cx, window, &view, RemoveMatchingLines);
    run(cx, window, &view, BookmarkMatchingLines);
    run(cx, window, &view, CopyMatchingLines);
    assert_eq!(editor_text(cx, &view), "a\nb\n");
    assert!(bookmarks(cx, &view).is_empty());
    assert_eq!(clipboard(cx), None);
    assert_eq!(
        text_of(cx, window, "editor-bar-error"),
        "Type the text to find."
    );
}

#[gpui_kit::test]
fn the_filter_lines_menu_removes_lines_by_mouse(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "keep\ndrop\n");
    find(cx, window, &view, "drop");
    cx.update_window(window, |_, window, cx| {
        window.click("find-filter-lines", cx)
    })
    .expect("window");
    settle(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.click(("filter-lines-row", 1_usize), cx);
    })
    .expect("window");
    settle(cx, &view);
    assert_eq!(editor_text(cx, &view), "keep\n");
}

#[gpui_kit::test]
fn a_text_over_1_mib_is_filtered_in_the_background(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "ab");
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let big = "ab\nxy\n".repeat(1 << 18);
    cx.update_window(window, |_, window, cx| {
        editor.update(cx, |state, cx| state.replace_all(big, window, cx));
    })
    .expect("window");
    find(cx, window, &view, "xy");
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(RemoveMatchingLines), cx);
    })
    .expect("window");
    assert_eq!(editor_text(cx, &view).len(), 6 << 18);
    cx.run_until_parked();
    assert_eq!(editor_text(cx, &view), "ab\n".repeat(1 << 18));
}
