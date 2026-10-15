use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::paging::hex_row_start;

use super::find::{bottom_keys, line_start, numbered_lines};
use super::{
    Setup, editor_of, editor_text, has_editor, kind, mixed, paged_window, press_only, text_window,
    top, visible, window_on,
};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, type_in};
use crate::window_view::viewer::PagedKind;

fn title(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    view.read_with(cx, |view, _| {
        view.viewer
            .as_ref()
            .map(|viewer| viewer.title.to_string())
            .unwrap_or_default()
    })
}

fn cursor(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u64 {
    view.read_with(cx, |view, _| {
        view.browser.state().active_tab().view().cursor
    })
}

fn search(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    press(cx, window, view, "ctrl-f");
    type_in(cx, window, view, "needle");
    press(cx, window, view, "enter");
    press(cx, window, view, "escape");
}

#[gpui_kit::test]
fn one_and_three_switch_between_text_and_hex_while_reading(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    assert!(has_editor(cx, &view));
    press(cx, window, &view, "3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    press(cx, window, &view, "5");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    press(cx, window, &view, "1");
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "hello\n");
}

#[gpui_kit::test]
fn while_editing_three_types_a_three(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "3");
    assert!(has_editor(cx, &view));
    assert_eq!(editor_text(cx, &view), "3hello\n");
    press(cx, window, &view, "alt-3");
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn f3_repeats_the_last_search_in_paged_text_and_shift_f3_goes_back(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let (line_100, line_900) = (line_start(&bytes, 100), line_start(&bytes, 900));
    let (window, view) = paged_window(cx, Setup::huge(bytes));
    search(cx, window, &view);
    assert_eq!(top(cx, &view), line_100);
    press(cx, window, &view, "f3");
    assert_eq!(top(cx, &view), line_900);
    press(cx, window, &view, "shift-f3");
    assert_eq!(top(cx, &view), line_100);
}

#[gpui_kit::test]
fn f3_repeats_the_last_search_in_hex(cx: &mut TestAppContext) {
    let mut bytes = mixed(64 * 1024);
    for at in [1000, 40_000] {
        bytes[at..at + 2].copy_from_slice(&[0x4D, 0x5A]);
    }
    let (window, view) = paged_window(cx, Setup::new(bytes));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "4D 5A");
    press(cx, window, &view, "enter");
    press(cx, window, &view, "escape");
    assert_eq!(top(cx, &view), hex_row_start(1000));
    press(cx, window, &view, "f3");
    assert_eq!(top(cx, &view), hex_row_start(40_000));
    press(cx, window, &view, "shift-f3");
    assert_eq!(top(cx, &view), hex_row_start(1000));
}

#[gpui_kit::test]
fn f3_before_any_search_opens_find(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(mixed(4096)));
    press(cx, window, &view, "f3");
    assert!(visible(cx, window, "paged-bar"));
}

fn search_session(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> (bool, usize, usize) {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| {
        let session = state.search_session();
        let current = session.matcher.current().unwrap_or(usize::MAX);
        (session.open, session.matcher.len(), current)
    })
}

#[gpui_kit::test]
fn f3_in_the_text_editor_opens_its_search_then_steps_through_matches(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one\ntwo\none\n".to_vec()));
    press(cx, window, &view, "f3");
    assert!(search_session(cx, &view).0);
    type_in(cx, window, &view, "one");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "f3");
    let (_, count, first) = search_session(cx, &view);
    assert_eq!(count, 2);
    press(cx, window, &view, "f3");
    let (_, _, second) = search_session(cx, &view);
    assert_ne!(first, second);
    press(cx, window, &view, "shift-f3");
    assert_eq!(search_session(cx, &view).2, first);
}

#[gpui_kit::test]
fn n_and_p_open_the_neighbour_files_and_skip_folders(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_file("home/b.txt", 4);
    fs.add_file("home/c.txt", 5);
    let (window, view, log) = window_on(cx, fs, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    let first = cursor(cx, &view);
    press(cx, window, &view, "n");
    assert_eq!(
        (title(cx, &view), cursor(cx, &view)),
        ("b.txt".to_owned(), first + 1)
    );
    press(cx, window, &view, "n");
    press(cx, window, &view, "p");
    assert_eq!(
        (title(cx, &view), cursor(cx, &view)),
        ("b.txt".to_owned(), first + 1)
    );
    press(cx, window, &view, "p");
    press(cx, window, &view, "p");
    assert_eq!(
        (title(cx, &view), cursor(cx, &view)),
        ("a.txt".to_owned(), first)
    );
    assert_eq!(log.heads(), ["a.txt", "b.txt", "c.txt", "b.txt", "a.txt"]);
}

#[gpui_kit::test]
fn the_bottom_bar_shows_the_lister_keys_then_the_editing_keys(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"hello\n".to_vec()));
    let reading = bottom_keys(cx, window);
    for key in [
        "Ctrl+E Edit",
        "3 Hex",
        "F3 Next",
        "N Next file",
        "P Previous file",
    ] {
        assert!(reading.contains(&key.to_owned()), "{key} in {reading:?}");
    }
    press(cx, window, &view, "ctrl-e");
    let editing = bottom_keys(cx, window);
    assert!(editing.contains(&"Ctrl+S Save".to_owned()));
    assert!(editing.contains(&"Alt+3 Hex".to_owned()));
    assert!(!editing.contains(&"N Next file".to_owned()));
}

#[gpui_kit::test]
fn enter_finds_the_next_match_while_reading_paged_text(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let (line_100, line_900) = (line_start(&bytes, 100), line_start(&bytes, 900));
    let (window, view) = paged_window(cx, Setup::huge(bytes));
    search(cx, window, &view);
    assert_eq!(top(cx, &view), line_100);
    press(cx, window, &view, "enter");
    assert_eq!(top(cx, &view), line_900);
}

#[gpui_kit::test]
fn enter_finds_next_in_the_read_only_editor_and_breaks_lines_while_editing(
    cx: &mut TestAppContext,
) {
    let (window, view, _) = text_window(cx, Setup::new(b"one\ntwo\none\n".to_vec()));
    press(cx, window, &view, "f3");
    type_in(cx, window, &view, "one");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "f3");
    let first = search_session(cx, &view).2;
    press(cx, window, &view, "enter");
    assert_ne!(search_session(cx, &view).2, first);
    assert_eq!(editor_text(cx, &view), "one\ntwo\none\n");
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "enter");
    assert_ne!(editor_text(cx, &view), "one\ntwo\none\n");
}

#[gpui_kit::test]
fn a_search_waits_for_queued_moves_before_it_starts(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let line_900 = line_start(&bytes, 900);
    let (window, view) = paged_window(cx, Setup::huge(bytes));
    search(cx, window, &view);
    press_only(cx, window, "ctrl-end");
    press_only(cx, window, "shift-f3");
    crate::window_view::tests::settle(cx, &view);
    super::redraw(cx, window);
    assert_eq!(top(cx, &view), line_900);
}
