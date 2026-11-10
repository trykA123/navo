//! The file diff body (V15): next and previous change, a narrow window in unified layout,
//! and a 1,000,000-row pair that builds only the visible rows.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, px};

use super::{Rig, rig, rig_sized};
use crate::compare_diff::ViewedKey;
use crate::compare_diff::{DiffRow, DiffUpdate, HunkView, Layout, PairFacts, PairKind, PairText};
use crate::tokens;

/// A pair of `len` equal rows, with a changed row (one hunk) at each of `changed`.
pub(super) fn pair(len: u32, changed: &[u32]) -> PairText {
    let rows = (0..len)
        .map(|at| match changed.contains(&at) {
            true => DiffRow::Changed(at, at),
            false => DiffRow::Equal(at, at),
        })
        .collect();
    let hunks = changed
        .iter()
        .map(|&at| HunkView {
            rows: at..at + 1,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 1,
            removed: 1,
        })
        .collect();
    let lines = vec![String::new(); len as usize];
    PairText {
        left: lines.clone(),
        right: lines,
        rows,
        hunks,
        facts: PairFacts {
            left_encoding: "cmd-viewer-encoding", // any real id: no encoding ids exist yet
            right_encoding: "cmd-viewer-encoding",
            left_endings: "compare-endings-lf",
            right_endings: "compare-endings-lf",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    }
}

impl Rig {
    /// Opens the selected row and delivers `text` as its opened pair.
    pub(super) fn open_text(&self, cx: &mut TestAppContext, text: PairText) {
        self.press(cx, "enter");
        let sink = self.diff.pair_sink();
        let opened = DiffUpdate::Opened {
            request: 1,
            kind: PairKind::Text,
            text: Some(Arc::new(text)),
        };
        self.push(cx, &sink, opened);
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    }

    /// The current change, the rows built in the last frame and the row count in `layout`.
    pub(super) fn probe(
        &self,
        cx: &mut TestAppContext,
        layout: Layout,
    ) -> (Option<usize>, Vec<usize>, usize) {
        self.view.update(cx, |view, _| {
            let (current, built, count) = view.file_probe(layout).expect("an opened text pair");
            (current, built.collect(), count)
        })
    }
}

#[gpui_kit::test]
fn compare_screen_next_and_previous_change_walk_the_hunks(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200, 390]));
    let layout = Layout::SideBySide;
    assert_eq!(rig.probe(cx, layout).0, None);
    rig.press(cx, "f7");
    assert_eq!(rig.probe(cx, layout).0, Some(0));
    rig.press(cx, "alt-down");
    let (current, built, _) = rig.probe(cx, layout);
    assert_eq!(current, Some(1));
    assert!(
        built.contains(&200),
        "the second change is shown: {built:?}"
    );
    rig.press(cx, "f7");
    rig.press(cx, "f7");
    let (current, built, _) = rig.probe(cx, layout);
    assert_eq!(current, Some(2), "stops at the last change");
    assert!(built.contains(&390));
    rig.press(cx, "shift-f7");
    rig.press(cx, "alt-up");
    let (current, built, _) = rig.probe(cx, layout);
    assert_eq!(current, Some(0));
    assert!(built.contains(&10));
}

#[gpui_kit::test]
fn compare_screen_is_unified_below_900_px(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig_sized(cx, px(700.));
    rig.open_text(cx, pair(6, &[2, 4]));
    let (_, built, count) = rig.probe(cx, Layout::Unified);
    assert_eq!(
        count, 8,
        "each changed row shows as a deleted and an inserted row"
    );
    assert_eq!(built, (0..8).collect::<Vec<_>>());
    rig.press(cx, "f7");
    rig.press(cx, "f7");
    assert_eq!(rig.probe(cx, Layout::Unified).0, Some(1));
}

#[gpui_kit::test]
fn compare_screen_scrolls_a_million_rows_building_only_the_viewport(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    let rows = 1_000_000;
    rig.open_text(cx, pair(rows, &[rows - 1]));
    rig.press(cx, "f7");
    let (current, built, count) = rig.probe(cx, Layout::SideBySide);
    assert_eq!(current, Some(0));
    assert_eq!(count, rows as usize);
    assert_eq!(built.last(), Some(&(count - 1)), "the last row is shown");
    let row_height = cx.update(|cx| tokens::current(cx).spacing.row_height);
    let viewport = (600. / f32::from(row_height)).ceil() as usize;
    assert!(
        built.len() <= viewport + 2,
        "built {} rows for a {viewport}-row viewport",
        built.len()
    );
}

#[gpui_kit::test]
fn compare_screen_map_jump_scrolls_and_the_current_change_follows_alt_down(
    cx: &mut TestAppContext,
) {
    use crate::compare_diff::map::MapEvent;

    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200, 390]));
    let map = rig
        .view
        .read_with(cx, |view, _| view.file_map())
        .expect("a mounted change map");
    map.update(cx, |_, cx| cx.emit(MapEvent::Jump(300)));
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    let (current, built, _) = rig.probe(cx, Layout::SideBySide);
    assert!(built.contains(&300), "the jump shows row 300: {built:?}");
    assert_eq!(current, None, "a jump does not change the current change");
    rig.press(cx, "alt-down");
    rig.press(cx, "alt-down");
    let shown = map.read_with(cx, |map, _| map.current());
    assert_eq!(shown, Some(1), "the map follows Alt+Down");
}

impl Rig {
    /// Renders two frames, waits past the 100 ms settle and returns the span requests so far.
    pub(super) fn settle_spans(&self, cx: &mut TestAppContext) -> Vec<std::ops::Range<u32>> {
        for _ in 0..2 {
            cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
                .expect("window");
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(150));
        cx.run_until_parked();
        self.diff.calls.spans.lock().unwrap().clone()
    }
}

#[gpui_kit::test]
fn compare_screen_asks_spans_only_for_unknown_rows_after_the_scroll_settles(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200, 390]));
    let first = rig.settle_spans(cx);
    assert_eq!(
        first.len(),
        1,
        "one request for the first screen: {first:?}"
    );
    assert_eq!(first[0].start, 0);
    rig.press(cx, "alt-down");
    rig.press(cx, "alt-down");
    let more = rig.settle_spans(cx);
    assert_eq!(more.len(), 2, "one more request after the scroll: {more:?}");
    assert!(
        more[1].start >= more[0].end,
        "known rows are not asked again: {more:?}"
    );
    assert!(
        more[1].contains(&200),
        "the rows around the second change: {more:?}"
    );
}

#[gpui_kit::test]
fn compare_screen_simple_look_asks_for_no_spans(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_text(cx, pair(400, &[10, 200]));
    rig.dispatch(cx, &crate::window_view::viewer::ViewSimple);
    rig.press(cx, "alt-down");
    assert!(rig.settle_spans(cx).is_empty());
}
