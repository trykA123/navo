use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, TestAppContext};
use rusty_core::marks::ticks;
use rusty_core::paging::{ByteSource, Needle};

use crate::prelude::*;
use crate::window_view::viewer::files::Reopen;
use crate::window_view::viewer::search::count::count_search;
use crate::window_view::viewer::search::marks::toggle_mark;
use crate::window_view::viewer::search::{Marked, Marking, source_changed};

use super::find::{line_start, numbered_lines};
use super::search::{editor_view, paged_marking, search_for};
use super::{Setup, label, redraw, text_of, text_window, top};
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
fn ctrl_1_while_editing_explains_that_the_edit_search_cannot_be_marked(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one two one\n".to_vec()));
    search_for(cx, window, &view, "one");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "ctrl-1");
    let editor = editor_view(cx, &view);
    let marks = editor.read_with(cx, |view, _| view.marking_state().marks.set.is_empty());
    assert!(marks);
    assert!(text_of(cx, window, "viewer-message").contains("Ctrl+1"));
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
fn switching_to_hex_clears_the_hit_ticks_of_the_text_search(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "escape");
    redraw(cx, window);
    let hits = |marking: &crate::window_view::viewer::search::Marking| {
        marking
            .ticks()
            .iter()
            .filter(|(_, lane)| *lane == 8)
            .count()
    };
    assert!(paged_marking(cx, &view, hits) > 0);
    press(cx, window, &view, "alt-3");
    redraw(cx, window);
    assert_eq!(paged_marking(cx, &view, hits), 0);
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
        "Ctrl+F2 Bookmark",
        "F2 Next bookmark",
        "Alt+F Follow",
        "Alt+V View",
        "Esc Close",
    ] {
        assert!(reading.contains(&key.to_owned()), "{key} in {reading:?}");
    }
    press(cx, window, &view, "ctrl-f");
    let finding = super::find::bottom_keys(cx, window);
    assert!(finding.contains(&"Alt+Enter Find all".to_owned()));
    assert!(finding.contains(&"Up History".to_owned()));
}

struct Probe(Marking);

impl Marked for Probe {
    fn marking(&mut self) -> &mut Marking {
        &mut self.0
    }

    fn jump(&mut self, _: Range<u64>, _: &mut Window, _: &mut Context<Self>) {}

    fn refocus(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn counted_source(text: &str, reads: &Arc<AtomicUsize>) -> Reopen {
    let (bytes, reads) = (Arc::new(text.as_bytes().to_vec()), reads.clone());
    Arc::new(move || {
        Ok(Box::new(super::Memory {
            bytes: bytes.clone(),
            changed: AtomicBool::new(false),
            reads: reads.clone(),
        }) as Box<dyn ByteSource + Send>)
    })
}

#[gpui_kit::test]
fn an_edit_drops_the_running_scans_of_the_old_text(cx: &mut TestAppContext) {
    let (old_reads, new_reads) = (Arc::default(), Arc::default());
    let probe = cx.new(|_| Probe(Marking::new(counted_source("needle\n", &old_reads))));
    probe.update(cx, |probe, cx| {
        toggle_mark(probe, needle(), 2, cx).expect("room");
        count_search(probe, needle(), cx);
        probe.0.source = counted_source("an needle\n", &new_reads);
        source_changed(probe, None, cx);
    });
    cx.run_until_parked();
    assert_eq!(old_reads.load(Ordering::Relaxed), 0);
    let (marked, counted) = probe.read_with(cx, |probe, _| {
        let counted = probe.0.counted().map(|counted| counted.starts.clone());
        (probe.0.marks.ranges_in(0..u64::MAX), counted)
    });
    assert_eq!(marked, [(3..9, 2)]);
    assert_eq!(counted, Some(vec![3]));
}

#[gpui_kit::test]
fn ctrl_number_marks_in_the_viewer_and_does_not_switch_tabs(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::huge(numbered_lines()));
    view.update(cx, |view, _| {
        let _ = view
            .browser
            .command(&rusty_core::Command::PaneOpenTab { location: None });
    });
    let active = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            let state = view.browser.state();
            state
                .pane(state.focused_pane())
                .map(|p| (p.tabs().len(), p.active_index()))
        })
    };
    let before = active(cx);
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-2");
    assert_eq!(active(cx), before);
    let colour = paged_marking(cx, &view, |marking| marking.marks.set.colour_of(&needle()));
    assert_eq!(colour, Some(1));
}
