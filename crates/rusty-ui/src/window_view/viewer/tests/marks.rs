use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use rusty_core::marks::ticks;
use rusty_core::paging::Needle;

use super::find::{line_start, numbered_lines};
use super::search::{editor_view, paged_marking, search_for};
use super::{Setup, label, redraw, text_window, top};
use crate::window_view::tests::press;

const NEEDLE_AT: u64 = 11;
const NEEDLE_WIDTH: u64 = 6;

fn shown(cx: &mut TestAppContext, window: AnyWindowHandle, id: (&'static str, usize)) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

fn needle() -> Needle {
    Needle::Text {
        text: "needle".to_owned(),
        match_case: false,
    }
}

fn needle_ranges() -> Vec<(std::ops::Range<u64>, u8)> {
    let bytes = numbered_lines();
    [100, 900]
        .map(|line| line_start(&bytes, line) + NEEDLE_AT)
        .into_iter()
        .map(|at| (at..at + NEEDLE_WIDTH, 2))
        .collect()
}

#[gpui_kit::test]
fn ctrl_3_marks_the_search_in_the_paged_rows(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    let marked = paged_marking(cx, &view, |marking| marking.marks.ranges_in(0..u64::MAX));
    assert_eq!(marked, needle_ranges());
    let shown_top = top(cx, &view);
    let on_screen = paged_marking(cx, &view, |marking| {
        marking.marks.ranges_in(shown_top..shown_top + 4096)
    });
    assert_eq!(on_screen, needle_ranges()[..1]);
}

#[gpui_kit::test]
fn the_same_colour_again_removes_the_mark_and_ctrl_0_clears_all(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "ctrl-3");
    let colour = paged_marking(cx, &view, |marking| marking.marks.set.colour_of(&needle()));
    assert_eq!(colour, None);
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "ctrl-5");
    let colour = paged_marking(cx, &view, |marking| marking.marks.set.colour_of(&needle()));
    assert_eq!(colour, Some(4));
    press(cx, window, &view, "ctrl-0");
    let empty = paged_marking(cx, &view, |marking| marking.marks.set.is_empty());
    assert!(empty);
}

#[gpui_kit::test]
fn ctrl_3_marks_the_search_in_the_text_editor(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one two one\nthree one\n".to_vec()));
    search_for(cx, window, &view, "one");
    press(cx, window, &view, "ctrl-3");
    redraw(cx, window);
    let editor = editor_view(cx, &view);
    let ranges = editor.read_with(cx, |view, cx| view.decorated_ranges(cx));
    assert_eq!(ranges, [0..3, 8..11, 18..21]);
    press(cx, window, &view, "ctrl-0");
    redraw(cx, window);
    let ranges = editor.read_with(cx, |view, cx| view.decorated_ranges(cx));
    assert!(ranges.is_empty());
}

#[gpui_kit::test]
fn the_overview_strip_shows_marks_and_hits_at_their_rows(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let len = bytes.len() as u64;
    let (window, view, _) = text_window(cx, Setup::huge(bytes));
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    redraw(cx, window);
    let (rows, shown) = paged_marking(cx, &view, |marking| {
        (marking.tick_rows(), marking.ticks().to_vec())
    });
    assert!(rows > 100, "the strip has {rows} rows");
    let spans: Vec<(u64, u8)> = needle_ranges()
        .into_iter()
        .flat_map(|(range, _)| [(range.start, 2), (range.start, 8)])
        .collect();
    assert_eq!(shown, ticks(&spans, len, rows));
}

#[gpui_kit::test]
fn ctrl_f2_toggles_a_bookmark_and_f2_steps_through_them_with_wrap(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    press(cx, window, &view, "ctrl-f2");
    assert!(shown(cx, window, ("paged-bookmark", 0)));
    press(cx, window, &view, "pagedown");
    press(cx, window, &view, "pagedown");
    let second = top(cx, &view);
    press(cx, window, &view, "ctrl-f2");
    press(cx, window, &view, "ctrl-home");
    press(cx, window, &view, "f2");
    assert_eq!(top(cx, &view), second);
    press(cx, window, &view, "f2");
    assert_eq!(top(cx, &view), 0);
    press(cx, window, &view, "shift-f2");
    assert_eq!(top(cx, &view), second);
    press(cx, window, &view, "ctrl-f2");
    let left = paged_marking(cx, &view, |marking| {
        marking.bookmarks.iter().collect::<Vec<_>>()
    });
    assert_eq!(left, [0]);
}

#[gpui_kit::test]
fn bookmarks_in_the_text_editor_follow_the_cursor_line(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one\ntwo\nthree\n".to_vec()));
    press(cx, window, &view, "ctrl-f2");
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    press(cx, window, &view, "ctrl-f2");
    let editor = editor_view(cx, &view);
    let marked = editor.read_with(cx, |view, _| {
        view.marking_state().bookmarks.iter().collect::<Vec<_>>()
    });
    assert_eq!(marked, [0, 8]);
    press(cx, window, &view, "f2");
    assert_eq!(editor.read_with(cx, |view, cx| view.selected(cx)), 0..0);
    press(cx, window, &view, "shift-f2");
    assert_eq!(editor.read_with(cx, |view, cx| view.selected(cx)), 8..8);
}

#[gpui_kit::test]
fn alt_l_shows_and_hides_line_endings_in_paged_text(cx: &mut TestAppContext) {
    let mut bytes = b"one\r\ntwo\n".to_vec();
    bytes.extend(numbered_lines());
    let (window, view, _) = text_window(cx, Setup::huge(bytes));
    assert!(!shown(cx, window, ("paged-eol", 0)));
    press(cx, window, &view, "alt-l");
    assert_eq!(label(cx, window, ("paged-eol", 0)), "CRLF");
    assert_eq!(label(cx, window, ("paged-eol", 1)), "LF");
    press(cx, window, &view, "alt-l");
    assert!(!shown(cx, window, ("paged-eol", 0)));
}

#[gpui_kit::test]
fn alt_l_toggles_line_endings_in_the_read_only_editor(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one\r\ntwo\n".to_vec()));
    let editor = editor_view(cx, &view);
    press(cx, window, &view, "alt-l");
    assert!(editor.read_with(cx, |view, _| view.marking_state().line_ends));
    press(cx, window, &view, "alt-l");
    assert!(!editor.read_with(cx, |view, _| view.marking_state().line_ends));
}

#[gpui_kit::test]
fn the_keys_bar_lists_the_mark_bookmark_and_follow_keys(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    let reading = super::find::bottom_keys(cx, window);
    for key in [
        "Ctrl+1…8 Mark",
        "Ctrl+F2 Bookmark",
        "F2 Next bookmark",
        "Alt+L Line ends",
        "Alt+F Follow",
        "Esc Close",
    ] {
        assert!(reading.contains(&key.to_owned()), "{key} in {reading:?}");
    }
    press(cx, window, &view, "ctrl-f");
    let finding = super::find::bottom_keys(cx, window);
    assert!(finding.contains(&"Alt+Enter Find all".to_owned()));
    assert!(finding.contains(&"Up History".to_owned()));
}
