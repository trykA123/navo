//! Headless tests for the editor's find bar in edit mode: Ctrl+F opens Navo's bar, its plain
//! replace is literal, Alt+R turns on regex mode, and Replace all is one undo step.

use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{editor_of, editor_text, press_only, text_of, visible, window_with};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{fake, press, type_in};
use crate::window_view::viewer::{FileHooks, FileText, Head, Kinds};

/// Opens a window whose selected file reads as `text`, views it with F3 and starts editing.
fn editing(cx: &mut TestAppContext, text: &'static str) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let files = FileHooks {
        head: Arc::new(move |_| {
            Ok(Head {
                bytes: text.as_bytes().to_vec(),
                len: text.len() as u64,
            })
        }),
        read: Arc::new(move |_| Ok(FileText::plain(text.to_owned()))),
        ..FileHooks::none()
    };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-e");
    (window, view)
}

/// Types `find` and `with` into the bar (Ctrl+H focuses the replace field), then
/// presses Ctrl+Alt+Enter for Replace all.
fn replace_all(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    (find, with): (&str, &str),
) {
    press(cx, window, view, "ctrl-f");
    type_in(cx, window, view, find);
    press(cx, window, view, "ctrl-h");
    type_in(cx, window, view, with);
    press(cx, window, view, "ctrl-alt-enter");
}

#[gpui_kit::test]
fn ctrl_f_in_edit_mode_opens_navos_bar_not_the_widget_panel(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "a-a-a");
    press(cx, window, &view, "ctrl-f");
    assert!(visible(cx, window, "editor-bar"));
    let editor = editor_of(cx, &view).expect("the text editor is open");
    assert!(!editor.read_with(cx, |state, _| state.search_session().open));
}

#[gpui_kit::test]
fn replace_all_swaps_every_plain_match(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "a-a-a");
    replace_all(cx, window, &view, ("a", "bb"));
    assert_eq!(editor_text(cx, &view), "bb-bb-bb");
}

#[gpui_kit::test]
fn one_undo_after_replace_all_restores_the_text(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "a-a-a");
    replace_all(cx, window, &view, ("a", "bb"));
    assert_eq!(editor_text(cx, &view), "bb-bb-bb");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-z");
    assert_eq!(editor_text(cx, &view), "a-a-a");
}

#[gpui_kit::test]
fn a_dollar_in_a_plain_replacement_stays_literal(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "x a x");
    replace_all(cx, window, &view, ("a", "$1"));
    assert_eq!(editor_text(cx, &view), "x $1 x");
}

/// Like `replace_all`, with regex mode turned on (Alt+R) before typing the pattern.
fn regex_replace_all(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    (find, with): (&str, &str),
) {
    press(cx, window, view, "ctrl-f");
    press(cx, window, view, "alt-r");
    replace_all(cx, window, view, (find, with));
}

#[gpui_kit::test]
fn a_regex_replacement_expands_numbered_groups(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "ann@home bob@work");
    regex_replace_all(cx, window, &view, (r"(\w+)@(\w+)", "$2 at $1"));
    assert_eq!(editor_text(cx, &view), "home at ann work at bob");
}

#[gpui_kit::test]
fn one_undo_after_a_regex_replace_all_restores_the_text(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "ann@home bob@work");
    regex_replace_all(cx, window, &view, (r"(\w+)@(\w+)", "$2 at $1"));
    assert_eq!(editor_text(cx, &view), "home at ann work at bob");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-z");
    assert_eq!(editor_text(cx, &view), "ann@home bob@work");
}

#[gpui_kit::test]
fn a_too_big_regex_says_so_and_finds_nothing(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "aaaa");
    press(cx, window, &view, "ctrl-f");
    press(cx, window, &view, "alt-r");
    type_in(cx, window, &view, r"\w{1000}{1000}");
    press(cx, window, &view, "enter");
    assert_eq!(
        text_of(cx, window, "editor-bar-error"),
        "Pattern is too complex"
    );
    let editor = editor_of(cx, &view).expect("the text editor is open");
    assert!(editor.read_with(cx, |state, _| state.selected_range().is_empty()));
}

#[gpui_kit::test]
fn the_edit_mode_count_reads_the_live_text(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "");
    type_in(cx, window, &view, "a-a");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "a");
    press(cx, window, &view, "enter");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 2");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-end");
    type_in(cx, window, &view, "-a");
    assert_eq!(editor_text(cx, &view), "a-a-a");
    press(cx, window, &view, "f3");
    let count = text_of(cx, window, "find-count");
    assert!(count.ends_with(" of 3"), "{count}");
}

#[gpui_kit::test]
fn a_caret_regex_replaces_at_every_line_start(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "x\ny");
    regex_replace_all(cx, window, &view, ("^", "> "));
    assert_eq!(editor_text(cx, &view), "> x\n> y");
}

/// Turns on regex mode, types `pattern` and presses Enter for one Find next without
/// letting background work run, so the count is still pending afterwards.
fn regex_step_only(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    pattern: &str,
) {
    press(cx, window, view, "ctrl-f");
    press(cx, window, view, "alt-r");
    type_in(cx, window, view, pattern);
    press_only(cx, window, "enter");
}

#[gpui_kit::test]
fn a_regex_find_next_on_6_mib_counts_in_the_background(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "ab");
    let editor = editor_of(cx, &view).expect("the text editor is open");
    let big = "ab\n".repeat(1 << 21);
    cx.update_window(window, |_, window, cx| {
        editor.update(cx, |state, cx| state.replace_all(big, window, cx));
    })
    .expect("window");
    regex_step_only(cx, window, &view, ".");
    assert_eq!(
        editor.read_with(cx, |state, _| state.selected_range()),
        0..1
    );
    assert_eq!(text_of(cx, window, "find-count"), "");
    cx.run_until_parked();
    assert_eq!(text_of(cx, window, "find-count"), "1 of 100,000+");
}

#[gpui_kit::test]
fn a_regex_count_stops_at_100_000_matches(cx: &mut TestAppContext) {
    let (window, view) = editing(cx, "a ".repeat(150_000).leak());
    regex_step_only(cx, window, &view, "a");
    cx.run_until_parked();
    assert_eq!(text_of(cx, window, "find-count"), "1 of 100,000+");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-end");
    press(cx, window, &view, "shift-f3");
    assert_eq!(text_of(cx, window, "find-count"), "100,000+ matches");
}
