use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::paging::{ByteSource, LineIndex};

use super::find::{line_start, numbered_lines};
use super::search::{paged_marking, search_for};
use super::{EDIT_LIMIT, Setup, hooks, paged, redraw, text_of, top, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in};
use crate::window_view::viewer::{FileHooks, Head, Kinds, Paged, PagedKind};

#[derive(Clone, Default)]
struct Growing {
    bytes: Arc<Mutex<Vec<u8>>>,
    changed: Arc<AtomicBool>,
    rewritten: Arc<AtomicBool>,
    scanned: Arc<AtomicU64>,
    is_scan: bool,
}

impl Growing {
    fn append(&self, more: &[u8]) {
        self.bytes.lock().expect("bytes").extend_from_slice(more);
        self.changed.store(true, Ordering::Relaxed);
    }

    fn rewrite(&self, bytes: &[u8], is_reported: bool) {
        *self.bytes.lock().expect("bytes") = bytes.to_vec();
        self.rewritten.store(is_reported, Ordering::Relaxed);
        self.changed.store(true, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Vec<u8> {
        self.bytes.lock().expect("bytes").clone()
    }

    fn scanned_bytes(&self) -> u64 {
        self.scanned.swap(0, Ordering::Relaxed)
    }
}

impl ByteSource for Growing {
    fn len(&self) -> u64 {
        self.bytes.lock().expect("bytes").len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let read = self
            .bytes
            .lock()
            .expect("bytes")
            .as_slice()
            .read_at(offset, buf)?;
        if self.is_scan {
            self.scanned.fetch_add(read as u64, Ordering::Relaxed);
        }
        Ok(read)
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }

    fn take_rewritten(&self) -> bool {
        self.rewritten.swap(false, Ordering::Relaxed)
    }
}

fn growing_paged(growing: &Growing) -> Paged {
    let reopened = Growing {
        is_scan: true,
        ..growing.clone()
    };
    let indexed = growing.clone();
    Paged {
        source: Arc::new(growing.clone()),
        reopen: Arc::new(move || Ok(Box::new(reopened.clone()) as Box<dyn ByteSource + Send>)),
        index: Arc::new(move |_, _| {
            let mut index = LineIndex::new();
            index.feed(&indexed.snapshot());
            index.finish();
            Ok(Some(index))
        }),
        summary: None,
        problem: None,
        editable: false,
        stamp: None,
    }
}

fn growing_hooks(growing: &Growing) -> FileHooks {
    let (source, head) = (growing.clone(), growing.clone());
    FileHooks {
        head: Arc::new(move |_| {
            Ok(Head {
                bytes: head.snapshot().into_iter().take(8 * 1024).collect(),
                len: EDIT_LIMIT + 1,
            })
        }),
        paged: Arc::new(move |_| Ok(growing_paged(&source))),
        ..hooks(Setup::new(Vec::new()), &super::Log::default())
    }
}

fn tick(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, view);
    redraw(cx, window);
    redraw(cx, window);
}

fn more_lines(from: usize) -> Vec<u8> {
    (from..from + 200)
        .map(|line| format!("line {line:05}\n"))
        .collect::<String>()
        .into_bytes()
}

#[gpui_kit::test]
fn follow_keeps_the_view_on_the_end_until_a_scroll_up_pauses_it(cx: &mut TestAppContext) {
    let growing = Growing::default();
    growing.append(&numbered_lines());
    growing.changed.store(false, Ordering::Relaxed);
    let (window, view) = window_with(cx, fake(), growing_hooks(&growing), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-f");
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    let at_end = top(cx, &view);
    assert!(at_end > 0);
    growing.append(&more_lines(2001));
    tick(cx, window, &view);
    let grown = top(cx, &view);
    assert!(grown > at_end, "{grown} stayed at {at_end}");
    press(cx, window, &view, "up");
    assert!(text_of(cx, window, "paged-follow").starts_with("Paused"));
    let paused = top(cx, &view);
    growing.append(&more_lines(2201));
    tick(cx, window, &view);
    assert_eq!(top(cx, &view), paused);
}

fn needle_lines(from: usize) -> Vec<u8> {
    (from..from + 200)
        .map(|line| format!("line {line:05} needle\n"))
        .collect::<String>()
        .into_bytes()
}

fn scans(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> (usize, Option<u64>) {
    paged_marking(cx, view, |marking| {
        let total = marking.counted().filter(|counted| counted.done);
        (marking.running_scans(), total.map(|counted| counted.total))
    })
}

fn redraw_grown(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> usize {
    paged(cx, view).update(cx, |_, cx| cx.notify());
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    scans(cx, view).0
}

#[gpui_kit::test]
fn growth_while_following_scans_only_the_new_bytes_with_two_scans_at_most(cx: &mut TestAppContext) {
    let growing = Growing::default();
    growing.append(&numbered_lines());
    let original = growing.len();
    growing.changed.store(false, Ordering::Relaxed);
    let (window, view) = window_with(cx, fake(), growing_hooks(&growing), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-f");
    for (text, mark) in [("line", "ctrl-1"), ("000", "ctrl-2"), ("needle", "ctrl-3")] {
        search_for(cx, window, &view, text);
        press(cx, window, &view, mark);
        press(cx, window, &view, "escape");
    }
    assert_eq!(scans(cx, &view), (0, Some(2)));
    let _ = growing.scanned_bytes();
    for from in [2001, 2201] {
        growing.append(&needle_lines(from));
        let running = redraw_grown(cx, window, &view);
        assert_eq!(running, 2);
    }
    while cx.executor().tick() {
        let (running, _) = scans(cx, &view);
        assert!(running <= 2, "{running} scans at once");
    }
    settle(cx, &view);
    tick(cx, window, &view);
    assert_eq!(scans(cx, &view), (0, Some(402)));
    let line_marks = paged_marking(cx, &view, |marking| {
        marking
            .marks
            .ranges_in(0..u64::MAX)
            .iter()
            .filter(|(_, colour)| *colour == 0)
            .count()
    });
    assert_eq!(line_marks, 2400);
    let grown = growing.len() - original;
    let scanned = growing.scanned_bytes();
    let four_tails = 4 * (grown + 8);
    assert!(
        scanned <= four_tails,
        "scanned {scanned} bytes for {grown} new"
    );
}

#[gpui_kit::test]
fn a_find_jump_pauses_follow_and_the_view_stays_on_the_hit(cx: &mut TestAppContext) {
    let growing = Growing::default();
    growing.append(&numbered_lines());
    growing.changed.store(false, Ordering::Relaxed);
    let (window, view) = window_with(cx, fake(), growing_hooks(&growing), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-f");
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "needle");
    press(cx, window, &view, "shift-enter");
    press(cx, window, &view, "escape");
    let hit = line_start(&numbered_lines(), 900);
    let on_hit = top(cx, &view);
    assert!(on_hit <= hit, "{on_hit} is past the hit at {hit}");
    assert!(text_of(cx, window, "paged-follow").starts_with("Paused"));
    growing.append(&more_lines(2001));
    tick(cx, window, &view);
    assert_eq!(top(cx, &view), on_hit);
}

fn found_rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> usize {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        (0..8_usize)
            .take_while(|row| {
                window
                    .within("find-all-list")
                    .try_find(("row", *row))
                    .is_some()
            })
            .count()
    })
    .expect("window")
}

#[gpui_kit::test]
fn find_all_lists_matches_that_arrive_while_following(cx: &mut TestAppContext) {
    let growing = Growing::default();
    growing.append(&numbered_lines());
    growing.changed.store(false, Ordering::Relaxed);
    let (window, view) = window_with(cx, fake(), growing_hooks(&growing), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-f");
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "needle");
    press(cx, window, &view, "alt-enter");
    assert_eq!(found_rows(cx, window), 2);
    growing.append(b"line 02001 needle\n");
    tick(cx, window, &view);
    tick(cx, window, &view);
    assert_eq!(found_rows(cx, window), 3);
}

fn total(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<u64> {
    paged_marking(cx, view, |marking| {
        marking
            .counted()
            .filter(|counted| counted.done)
            .map(|counted| counted.total)
    })
}

fn follow_and_count_needles(
    cx: &mut TestAppContext,
    growing: &Growing,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    growing.append(&numbered_lines());
    growing.changed.store(false, Ordering::Relaxed);
    let (window, view) = window_with(cx, fake(), growing_hooks(growing), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-f");
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "escape");
    assert_eq!(total(cx, &view), Some(2));
    (window, view)
}

#[gpui_kit::test]
fn a_followed_file_truncated_and_regrown_is_counted_again_from_the_start(cx: &mut TestAppContext) {
    let growing = Growing::default();
    let (window, view) = follow_and_count_needles(cx, &growing);
    let keys = super::find::bottom_keys(cx, window);
    assert!(
        keys.contains(&"Alt+F Stop following".to_owned()),
        "{keys:?}"
    );
    growing.rewrite(b"fresh start needle\n", false);
    tick(cx, window, &view);
    assert_eq!(total(cx, &view), Some(1));
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    growing.append(&needle_lines(1));
    tick(cx, window, &view);
    assert_eq!(total(cx, &view), Some(201));
}

#[gpui_kit::test]
fn a_followed_file_rewritten_longer_between_polls_is_counted_again_from_the_start(
    cx: &mut TestAppContext,
) {
    let growing = Growing::default();
    let (window, view) = follow_and_count_needles(cx, &growing);
    let mut replacement = more_lines(1);
    replacement.extend(numbered_lines());
    growing.rewrite(&replacement, true);
    tick(cx, window, &view);
    assert_eq!(total(cx, &view), Some(2));
    let hit_rows = paged_marking(cx, &view, |marking| {
        marking.counted().map(|counted| counted.starts.clone())
    });
    let offset = more_lines(1).len() as u64;
    let expected: Vec<u64> = [100, 900]
        .iter()
        .map(|line| offset + line_start(&numbered_lines(), *line) + 11)
        .collect();
    assert_eq!(hit_rows, Some(expected));
}

#[gpui_kit::test]
fn alt_f_in_the_read_only_editor_follows_in_paged_text_from_the_cursor_line(
    cx: &mut TestAppContext,
) {
    let bytes = numbered_lines();
    let (window, view, _) = super::text_window(cx, super::Setup::new(bytes.clone()));
    assert!(super::has_editor(cx, &view));
    let keys = super::find::bottom_keys(cx, window);
    assert!(keys.contains(&"Alt+F Follow".to_owned()), "{keys:?}");
    for _ in 0..3 {
        press(cx, window, &view, "down");
    }
    press(cx, window, &view, "alt-f");
    redraw(cx, window);
    assert_eq!(super::kind(cx, &view), Some(PagedKind::Text));
    assert_eq!(text_of(cx, window, "paged-follow"), "Following");
    assert_eq!(top(cx, &view), line_start(&bytes, 4));
}

#[gpui_kit::test]
fn alt_f_with_unsaved_edits_only_shows_the_note(cx: &mut TestAppContext) {
    let (window, view, _) = super::text_window(cx, super::Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    press(cx, window, &view, "alt-f");
    assert!(super::has_editor(cx, &view));
    assert_eq!(super::editor_text(cx, &view), "xhello\n");
    assert_eq!(
        text_of(cx, window, "viewer-message"),
        "Save or discard your edits to follow this file."
    );
}

#[gpui_kit::test]
fn alt_f_while_editing_without_edits_stays_in_the_editor(cx: &mut TestAppContext) {
    let (window, view, _) = super::text_window(cx, super::Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "alt-f");
    assert!(super::has_editor(cx, &view));
    assert_eq!(super::kind(cx, &view), None);
}
