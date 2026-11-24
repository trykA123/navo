//! Headless tests for the editor's find bar in edit mode: Ctrl+F opens Navo's bar, and its
//! plain replace changes the text literally, with Replace all as one undo step.

use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::{editor_of, editor_text, visible, window_with};
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
