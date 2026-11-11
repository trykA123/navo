//! The large text body under load (V38 review): rows far down a 10M-line pair, one line window in
//! flight, sides far apart, rows the diff has not settled, keys, failure and release.

use std::ops::Range;

use gpui_kit::{AppContext as _, Keystroke, TestAppContext};

use super::compare_large::{hunk, lines, open_large, push};
use super::compare_screen::Rig;
use super::settle;
use crate::compare_diff::{
    DiffUpdate, LargeHunk, LargeProgress, LargeStage, LargeStep, LargeStepReason, LargeStepView,
    ShownKind,
};
use crate::window_view::compare_screen::FileState;

/// A snapshot of request 1 with `hunks`, `steps` and sections `done` of `total`.
fn snapshot(
    hunks: Vec<LargeHunk>,
    steps: Vec<LargeStepView>,
    (done, total): (u64, u64),
) -> DiffUpdate {
    DiffUpdate::Large {
        request: 1,
        hunks,
        steps,
        progress: Some(LargeProgress {
            stage: LargeStage::Sections,
            done,
            total,
        }),
        ack: done,
    }
}

/// A section whose hunks were evicted (D17 paged hunks) over `left` lines.
fn paged(left: Range<u64>) -> LargeStepView {
    LargeStepView {
        step: LargeStep::PagedHunks,
        lines: left.end - left.start,
        section: left,
        reason: LargeStepReason::Paged,
    }
}

/// The top row and the left line it shows.
fn top(cx: &mut TestAppContext, rig: &Rig) -> (u64, Option<u64>) {
    let row = rig
        .view
        .read_with(cx, |view, _| view.large_probe())
        .expect("a large body")
        .top;
    let shown = rig.view.read_with(cx, |view, _| view.large_row(row));
    (row, shown.and_then(|(_, left, _)| left).map(|(at, _)| at))
}

/// The kind of row `row`.
fn kind(cx: &mut TestAppContext, rig: &Rig, row: u64) -> ShownKind {
    let shown = rig.view.read_with(cx, |view, _| view.large_row(row));
    shown.expect("a large body").0
}

/// Scrolls the large body so row `row` is on top.
fn scroll(cx: &mut TestAppContext, rig: &Rig, row: usize) {
    rig.view.update(cx, |view, cx| {
        view.large_scroll_to(row);
        cx.notify();
    });
    settle(cx, &rig.view);
}

/// Answers the line requests past the first `answered`; false when none was new.
fn answer(cx: &mut TestAppContext, rig: &Rig, answered: &mut usize, totals: (u64, u64)) -> bool {
    let sink = rig.diff.pair_sink();
    let asked = rig.diff.line_requests();
    let fresh = asked.get(*answered..).unwrap_or_default().to_vec();
    *answered = asked.len();
    for rows in &fresh {
        push(cx, rig, &sink, lines(rows.clone(), totals));
    }
    !fresh.is_empty()
}

#[gpui_kit::test]
fn compare_large_keeps_the_top_line_ten_million_rows_down(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    const TOTAL: u64 = 10_000_000;
    push(cx, &rig, &sink, lines(0..10, (TOTAL, TOTAL)));
    scroll(cx, &rig, 9_000_003);
    assert_eq!(top(cx, &rig), (9_000_003, Some(9_000_003)));
    // Two remaps before the next frame: ten rows arrive above, then five more.
    let first = vec![hunk(10..10, 10..20)];
    sink(snapshot(first, Vec::new(), (1, 5)));
    let both = vec![hunk(10..10, 10..20), hunk(20..20, 30..35)];
    sink(snapshot(both, Vec::new(), (2, 5)));
    settle(cx, &rig.view);
    assert_eq!(
        top(cx, &rig),
        (9_000_018, Some(9_000_003)),
        "the same line stays on top"
    );
    rig.view.update(cx, |_, cx| cx.notify());
    settle(cx, &rig.view);
    assert_eq!(
        top(cx, &rig),
        (9_000_018, Some(9_000_003)),
        "and stays after a frame"
    );
}

#[gpui_kit::test]
fn compare_large_keeps_one_line_window_in_flight(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _sink) = open_large(cx);
    let totals = (5_000, 5_000);
    let mut answered = 0;
    assert!(
        answer(cx, &rig, &mut answered, totals),
        "the first screen asks"
    );
    scroll(cx, &rig, 3_000);
    assert_eq!(rig.diff.line_requests().len(), 2, "the new view asks once");
    scroll(cx, &rig, 4_000);
    assert_eq!(
        rig.diff.line_requests().len(),
        2,
        "nothing more while one is in flight"
    );
    answer(cx, &rig, &mut answered, totals);
    let asked = rig.diff.line_requests();
    assert_eq!(asked.len(), 3, "the view in place now is asked: {asked:?}");
    assert!(asked[2].contains(&4_000), "{asked:?}");
}

#[gpui_kit::test]
fn compare_large_sides_far_apart_settle_without_a_request_loop(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    let totals = (30_000, 50_000);
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(100..100, 100..20_100)], Vec::new(), (5, 5)),
    );
    push(cx, &rig, &sink, lines(0..9_000, totals));
    let mut answered = rig.diff.line_requests().len();
    scroll(cx, &rig, 21_000);
    let rounds = (0..12)
        .take_while(|_| answer(cx, &rig, &mut answered, totals))
        .count();
    assert!(rounds < 4, "the requests settle: {rounds} rounds");
    let row = rig
        .view
        .read_with(cx, |view, _| view.large_row(21_000))
        .unwrap();
    let text = |line: Option<(u64, String)>| line.map(|(_, text)| text);
    assert_eq!(
        (text(row.1), text(row.2)),
        (Some("l1000".into()), Some("r21000".into()))
    );
}

#[gpui_kit::test]
fn compare_large_unsettled_rows_are_never_changes_or_equal(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..100, (1_000, 1_010)));
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(500..501, 500..501)], Vec::new(), (1, 5)),
    );
    let unsettled = |kind| {
        !matches!(
            kind,
            ShownKind::Equal | ShownKind::Changed | ShownKind::Deleted | ShownKind::Inserted
        )
    };
    assert!(
        unsettled(kind(cx, &rig, 700)),
        "a tail still diffing is no change"
    );
    assert!(
        unsettled(kind(cx, &rig, 50)),
        "nor are lines still diffing equal"
    );
    assert_eq!(kind(cx, &rig, 500), ShownKind::Changed);
    let evicted = vec![paged(0..400)];
    let hunks = vec![hunk(450..451, 450..451), hunk(500..501, 500..501)];
    push(cx, &rig, &sink, snapshot(hunks, evicted, (5, 5)));
    assert!(
        unsettled(kind(cx, &rig, 100)),
        "an evicted section is not equal"
    );
    assert_eq!(
        kind(cx, &rig, 470),
        ShownKind::Equal,
        "nor is the rest unsettled"
    );
    assert!(
        unsettled(kind(cx, &rig, 700)),
        "a tail of unequal length is no change"
    );
}

#[gpui_kit::test]
fn compare_large_scrolls_by_line_page_and_end_keys(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..100, (5_000, 5_000)));
    rig.press(cx, "down");
    assert_eq!(top(cx, &rig).0, 1);
    rig.press(cx, "pagedown");
    let paged = top(cx, &rig).0;
    assert!(paged > 10, "a page down: {paged}");
    rig.press(cx, "pageup");
    assert_eq!(top(cx, &rig).0, 1);
    rig.press(cx, "up");
    assert_eq!(top(cx, &rig).0, 0);
    rig.press(cx, "end");
    assert!(
        top(cx, &rig).0 > 4_900,
        "the last page: {:?}",
        top(cx, &rig)
    );
    rig.press(cx, "home");
    assert_eq!(top(cx, &rig).0, 0);
}

#[gpui_kit::test]
fn compare_large_ctrl_home_and_ctrl_end_jump_to_the_ends(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..100, (5_000, 5_000)));
    rig.press(cx, "ctrl-end");
    assert!(top(cx, &rig).0 > 4_900, "ctrl-end: {:?}", top(cx, &rig));
    rig.press(cx, "ctrl-home");
    assert_eq!(top(cx, &rig).0, 0, "ctrl-home goes to the first line");
}

#[gpui_kit::test]
fn compare_large_failure_and_refresh_drop_the_large_body(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(10..11, 10..11)], Vec::new(), (1, 5)),
    );
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    assert!(
        rig.view
            .read_with(cx, |view, _| view.large_probe().is_none()),
        "refresh drops it"
    );
    let (rig, sink) = open_large(cx);
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(10..11, 10..11)], Vec::new(), (1, 5)),
    );
    let text = "Could not read".to_owned();
    push(
        cx,
        &rig,
        &sink,
        DiffUpdate::Failed {
            request: 1,
            text: text.clone(),
        },
    );
    assert!(
        rig.view
            .read_with(cx, |view, _| view.large_probe().is_none())
    );
    assert_eq!(rig.state(cx), Some(FileState::Failed(text.into())));
}

#[gpui_kit::test]
fn compare_large_changed_on_disk_refreshes_and_drops_the_body(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(10..11, 10..11)], Vec::new(), (1, 5)),
    );
    push(cx, &rig, &sink, DiffUpdate::ChangedOnDisk { request: 1 });
    assert_eq!(
        rig.diff.refreshes(),
        vec![2],
        "the line read asks for a reload"
    );
    assert!(
        rig.view
            .read_with(cx, |view, _| view.large_probe().is_none()),
        "the stale body is gone"
    );
}

#[gpui_kit::test]
fn compare_large_release_drops_the_pair_before_the_next_frame(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(10..11, 10..11)], Vec::new(), (1, 5)),
    );
    cx.update_window(rig.window, |_, window, cx| {
        let escape = Keystroke::parse("escape").expect("keystroke");
        assert!(window.dispatch_keystroke(escape, cx));
    })
    .expect("window");
    assert!(
        rig.diff.dropped(),
        "closing frees the pair, not the next paint"
    );
}
