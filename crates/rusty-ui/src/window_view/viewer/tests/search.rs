use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::marks::SearchHistory;
use rusty_core::paging::Needle;

use super::find::{line_start, numbered_lines};
use super::{
    Log, Setup, hooks, mixed, paged, press_only, redraw, text_of, text_window, window_with,
};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in};
use crate::window_view::viewer::editor::{EditorBody, EditorView};
use crate::window_view::viewer::search::Marking;
use crate::window_view::viewer::{FileHooks, Kinds, PagedView};

pub(super) fn with_paged<R>(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    read: impl FnOnce(&PagedView, &gpui_kit::App) -> R,
) -> R {
    paged(cx, view).read_with(cx, |paged, cx| read(paged, cx))
}

pub(super) fn paged_marking<R>(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    read: impl FnOnce(&Marking) -> R,
) -> R {
    with_paged(cx, view, |paged, _| read(paged.marking_state()))
}

pub(super) fn editor_view(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
) -> Entity<EditorView> {
    view.read_with(cx, |view, _| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        let editor = body.as_any().downcast_ref::<EditorBody>()?;
        Some(editor.0.clone())
    })
    .expect("the text editor is open")
}

pub(super) fn type_only(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
}

pub(super) fn search_for(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    text: &str,
) {
    press(cx, window, view, "ctrl-f");
    type_in(cx, window, view, text);
    press(cx, window, view, "enter");
}

fn word(text: &str) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case: false,
    }
}

fn planted_hex() -> Vec<u8> {
    let mut bytes = mixed(64 * 1024);
    for at in [1000, 20_000, 40_000] {
        bytes[at..at + 2].copy_from_slice(&[0x4D, 0x5A]);
    }
    bytes
}

#[gpui_kit::test]
fn the_find_bar_counts_hex_matches_and_shows_the_place_of_the_hit(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(planted_hex()));
    search_for(cx, window, &view, "4D 5A");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 3");
    press(cx, window, &view, "enter");
    assert_eq!(text_of(cx, window, "find-count"), "2 of 3");
}

#[gpui_kit::test]
fn paged_text_counts_matches_in_the_find_bar(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    search_for(cx, window, &view, "needle");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 2");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "f3");
    assert_eq!(text_of(cx, window, "find-count"), "2 of 2");
}

#[gpui_kit::test]
fn the_text_editor_counts_matches_in_its_find_bar(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one two one\nthree one\n".to_vec()));
    search_for(cx, window, &view, "one");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 3");
    press(cx, window, &view, "enter");
    assert_eq!(text_of(cx, window, "find-count"), "2 of 3");
}

#[gpui_kit::test]
fn a_new_query_cancels_the_running_count(cx: &mut TestAppContext) {
    let (window, view, log) = text_window(cx, Setup::huge(numbered_lines()));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "line 0001");
    let before = log.reopened_reads();
    press_only(cx, window, "enter");
    type_only(cx, window, "0");
    press_only(cx, window, "enter");
    settle(cx, &view);
    redraw(cx, window);
    let replaced = log.reopened_reads() - before;
    assert_eq!(text_of(cx, window, "find-count"), "1 of 1");
    press(cx, window, &view, "backspace");
    type_in(cx, window, &view, "1");
    let before = log.reopened_reads();
    press(cx, window, &view, "enter");
    let alone = log.reopened_reads() - before;
    assert_eq!(text_of(cx, window, "find-count"), "1 of 1");
    assert_eq!(replaced, alone, "the first count kept reading");
}

#[gpui_kit::test]
fn a_hit_past_the_kept_starts_shows_only_the_total(cx: &mut TestAppContext) {
    let mut bytes = vec![b'a'; 150_000];
    bytes[0] = 0;
    let (window, view, _) = text_window(cx, Setup::new(bytes));
    press(cx, window, &view, "ctrl-end");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "61");
    press(cx, window, &view, "shift-enter");
    assert_eq!(text_of(cx, window, "find-count"), "of 149,999");
}

fn history_hooks(log: &Log, remembered: &Arc<Mutex<Vec<Needle>>>) -> FileHooks {
    let remembered = remembered.clone();
    FileHooks {
        history: Arc::new(|| {
            let mut history = SearchHistory::default();
            history.push(word("line 00500"));
            history.push(word("needle"));
            history
        }),
        remember: Arc::new(move |needle| {
            remembered.lock().expect("log").push(needle.clone());
        }),
        ..hooks(Setup::huge(numbered_lines()), log)
    }
}

fn bar_text(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    with_paged(cx, view, |paged, cx| paged.bar_text(cx)).unwrap_or_default()
}

#[gpui_kit::test]
fn up_and_down_in_the_find_field_walk_the_history(cx: &mut TestAppContext) {
    let remembered = Arc::default();
    let files = history_hooks(&Log::default(), &remembered);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "draft");
    press(cx, window, &view, "up");
    assert_eq!(bar_text(cx, &view), "needle");
    press(cx, window, &view, "up");
    assert_eq!(bar_text(cx, &view), "line 00500");
    press(cx, window, &view, "down");
    assert_eq!(bar_text(cx, &view), "needle");
    press(cx, window, &view, "down");
    assert_eq!(bar_text(cx, &view), "draft");
}

#[gpui_kit::test]
fn an_executed_search_is_remembered_through_the_hooks(cx: &mut TestAppContext) {
    let remembered: Arc<Mutex<Vec<Needle>>> = Arc::default();
    let files = history_hooks(&Log::default(), &remembered);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    search_for(cx, window, &view, "line 00700");
    assert_eq!(*remembered.lock().expect("log"), [word("line 00700")]);
    let line = line_start(&numbered_lines(), 700);
    assert_eq!(super::top(cx, &view), line);
}

fn pause(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>, ms: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(ms));
    settle(cx, view);
    redraw(cx, window);
}

#[gpui_kit::test]
fn the_paged_count_follows_the_typed_text_after_a_short_pause(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "need");
    pause(cx, window, &view, 100);
    type_in(cx, window, &view, "le");
    pause(cx, window, &view, 100);
    assert_eq!(text_of(cx, window, "find-count"), "");
    pause(cx, window, &view, 60);
    assert_eq!(text_of(cx, window, "find-count"), "of 2");
    type_in(cx, window, &view, "x");
    pause(cx, window, &view, 160);
    assert_eq!(text_of(cx, window, "find-count"), "No matches");
    press(cx, window, &view, "backspace");
    press(cx, window, &view, "enter");
    assert_eq!(text_of(cx, window, "find-count"), "1 of 2");
}

#[gpui_kit::test]
fn the_editor_count_follows_the_typed_text_and_clears_when_empty(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one two one\nthree one\n".to_vec()));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "one");
    pause(cx, window, &view, 160);
    assert_eq!(text_of(cx, window, "find-count"), "of 3");
    for _ in 0..3 {
        press(cx, window, &view, "backspace");
    }
    pause(cx, window, &view, 160);
    assert_eq!(text_of(cx, window, "find-count"), "");
}
