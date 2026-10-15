use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::paging::hex_row_start;

use super::{Setup, label, mixed, paged_window, redraw, top, visible};
use crate::window_view::tests::{press, type_in, viewer_open};

pub(super) fn numbered_lines() -> Vec<u8> {
    (1..=2000)
        .map(|line| match line {
            100 | 900 => format!("line {line:05} needle\n"),
            _ => format!("line {line:05}\n"),
        })
        .collect::<String>()
        .into_bytes()
}

pub(super) fn line_start(bytes: &[u8], line: usize) -> u64 {
    if line == 1 {
        return 0;
    }
    let newline = bytes
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .nth(line - 2)
        .map(|(at, _)| at)
        .expect("the line exists");
    newline as u64 + 1
}

fn status(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> String {
    cx.update_window(window, |_, window, _| {
        window
            .try_find("paged-status")
            .and_then(|found| found.label().map(str::to_owned))
            .unwrap_or_default()
    })
    .expect("window")
}

#[gpui_kit::test]
fn hex_search_finds_bytes_deep_in_a_large_source(cx: &mut TestAppContext) {
    let mut bytes = mixed(3 * 1024 * 1024);
    let at = 2_500_003;
    bytes[at..at + 2].copy_from_slice(&[0x4D, 0x5A]);
    let (window, view) = paged_window(cx, Setup::new(bytes));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "4D 5A");
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert_eq!(top(cx, &view), hex_row_start(at as u64));
}

#[gpui_kit::test]
fn bad_hex_shows_an_error_and_does_not_search(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(mixed(4096)));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "4G");
    press(cx, window, &view, "enter");
    assert!(visible(cx, window, "paged-bar-error"));
    assert_eq!(status(cx, window), "");
    assert_eq!(top(cx, &view), 0);
}

#[gpui_kit::test]
fn shift_enter_finds_the_previous_match(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let (line_900, line_100) = (line_start(&bytes, 900), line_start(&bytes, 100));
    let (window, view) = paged_window(cx, Setup::huge(bytes));
    press(cx, window, &view, "ctrl-end");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "NEEDLE");
    press(cx, window, &view, "shift-enter");
    assert_eq!(top(cx, &view), line_900);
    press(cx, window, &view, "shift-enter");
    assert_eq!(top(cx, &view), line_100);
}

#[gpui_kit::test]
fn a_missing_needle_is_not_found_below(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::huge(numbered_lines()));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "absent");
    press(cx, window, &view, "enter");
    assert_eq!(status(cx, window), "Not found below.");
    assert_eq!(top(cx, &view), 0);
}

#[gpui_kit::test]
fn the_index_numbers_lines_and_ctrl_g_goes_to_one(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let line_500 = line_start(&bytes, 500);
    let (window, view) = paged_window(cx, Setup::huge(bytes));
    assert_eq!(label(cx, window, ("paged-line", 0)), "1");
    assert_eq!(label(cx, window, ("paged-line", 1)), "2");
    press(cx, window, &view, "ctrl-g");
    type_in(cx, window, &view, "500");
    press(cx, window, &view, "enter");
    assert_eq!(top(cx, &view), line_500);
    assert_eq!(label(cx, window, ("paged-line", 0)), "500");
}

#[gpui_kit::test]
fn escape_closes_the_search_bar_before_the_viewer(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(mixed(4096)));
    press(cx, window, &view, "ctrl-f");
    assert!(visible(cx, window, "paged-bar"));
    press(cx, window, &view, "escape");
    assert!(!visible(cx, window, "paged-bar"));
    assert!(viewer_open(cx, &view));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

fn shown(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, id: &'static str) -> String {
    cx.update_window(window, |_, window, _| {
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
            .unwrap_or_default()
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_finished_index_shows_the_line_count(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let lines = bytes.iter().filter(|byte| **byte == b'\n').count()
        + usize::from(bytes.last() != Some(&b'\n'));
    let (window, _view) = paged_window(cx, Setup::huge(bytes));
    let count = shown(cx, window, "paged-index").replace(',', "");
    assert_eq!(count, format!("{lines} lines"));
}

#[gpui_kit::test]
fn a_failed_index_shows_its_error_instead_of_progress(cx: &mut TestAppContext) {
    let setup = Setup {
        index_fails: true,
        ..Setup::huge(numbered_lines())
    };
    let (window, _view) = paged_window(cx, setup);
    assert!(!visible(cx, window, "paged-index"));
    assert!(status(cx, window).contains("gone"));
}

pub(super) fn bottom_keys(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
) -> Vec<String> {
    (0usize..12)
        .filter_map(|index| {
            cx.update_window(window, |_, window, _| {
                window
                    .try_find(("fkey", index))
                    .and_then(|found| found.label().map(str::to_owned))
            })
            .ok()
            .flatten()
        })
        .collect()
}

#[gpui_kit::test]
fn the_bottom_bar_shows_the_viewer_keys_in_each_mode(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(mixed(4096)));
    assert!(bottom_keys(cx, window).contains(&"Ctrl+G Go to offset".to_owned()));
    press(cx, window, &view, "alt-1");
    let text = bottom_keys(cx, window);
    assert!(text.contains(&"Ctrl+G Go to line".to_owned()));
    assert!(text.contains(&"3 Hex".to_owned()));
    press(cx, window, &view, "ctrl-f");
    assert!(bottom_keys(cx, window).contains(&"Shift+Enter Previous".to_owned()));
}
