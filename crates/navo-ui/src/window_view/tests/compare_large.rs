//! The large text body of the file diff (V38, D16): rows built from hunks and requested line
//! windows, "Looking further… N %" past the known hunks, and a top line that arriving hunks
//! never move.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_l10n::t;

use super::compare_screen::{Rig, rig};
use super::settle;
use crate::compare_diff::{
    DiffSink, DiffUpdate, LargeHunk, LargeProgress, LargeStage, LargeStep, LargeStepReason,
    LargeStepView, Layout, PairKind, ShownKind,
};
use crate::window_view::compare_screen::large::{Anchor, RowMap, Settled};

/// A hunk over `left` and `right` lines; the byte ranges do not matter to the body.
pub(super) fn hunk(left: std::ops::Range<u64>, right: std::ops::Range<u64>) -> LargeHunk {
    LargeHunk {
        left_rows: left.clone(),
        right_rows: right.clone(),
        left_bytes: left,
        right_bytes: right,
    }
}

/// A snapshot of request 1 with `hunks`, sections `done` of 5 and acknowledgement `ack`.
fn snapshot(hunks: Vec<LargeHunk>, done: u64, ack: u64) -> DiffUpdate {
    DiffUpdate::Large {
        request: 1,
        hunks,
        steps: Vec::new(),
        progress: Some(LargeProgress {
            stage: LargeStage::Sections,
            done,
            total: 5,
        }),
        ack,
    }
}

/// Lines `rows` of both sides ("l7", "r7") with the sides' line counts.
pub(super) fn lines(rows: std::ops::Range<u64>, totals: (u64, u64)) -> DiffUpdate {
    DiffUpdate::LargeLines {
        request: 1,
        rows: rows.clone(),
        left: rows.clone().map(|at| format!("l{at}")).collect(),
        right: rows.map(|at| format!("r{at}")).collect(),
        left_cut: false,
        right_cut: false,
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        totals,
    }
}

/// Opens `a.txt` as a large pair and returns the sink its updates go through.
pub(super) fn open_large(cx: &mut TestAppContext) -> (Rig, DiffSink) {
    let rig = rig(cx);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Large {
            left: 1 << 30,
            right: 1 << 30,
        },
        text: None,
    });
    settle(cx, &rig.view);
    (rig, sink)
}

/// Opens the selected row of `rig` as a large pair; returns the sink its updates go through.
pub(super) fn open_large_in(cx: &mut TestAppContext, rig: &Rig) -> DiffSink {
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Large {
            left: 1 << 30,
            right: 1 << 30,
        },
        text: None,
    });
    settle(cx, &rig.view);
    sink
}

pub(super) fn push(cx: &mut TestAppContext, rig: &Rig, sink: &DiffSink, update: DiffUpdate) {
    sink(update);
    settle(cx, &rig.view);
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> String {
    let probe = rig.view.read_with(cx, |view, _| view.large_probe());
    probe.expect("a large body").status.to_string()
}

#[gpui_kit::test]
fn compare_large_rows_come_from_hunks_and_requested_line_windows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, snapshot(vec![hunk(10..12, 10..15)], 1, 1));
    let asked = rig.diff.line_requests();
    assert_eq!(asked.first().map(|rows| rows.start), Some(0), "{asked:?}");
    push(cx, &rig, &sink, lines(0..40, (30, 33)));
    let probe = rig
        .view
        .read_with(cx, |view, _| view.large_probe())
        .unwrap();
    assert_eq!(probe.count, 33, "10 equal, 5 in the hunk, 18 after it");
    let row = |cx: &mut TestAppContext, at| {
        rig.view
            .read_with(cx, |view, _| view.large_row(at))
            .unwrap()
    };
    let shown = |line: u64, text: &str| Some((line, text.to_owned()));
    assert_eq!(
        row(cx, 11),
        (ShownKind::Changed, shown(11, "l11"), shown(11, "r11"))
    );
    assert_eq!(row(cx, 13), (ShownKind::Inserted, None, shown(13, "r13")));
    assert_eq!(
        row(cx, 15),
        (ShownKind::Pending, shown(12, "l12"), shown(15, "r15"))
    );
    assert_eq!(status(cx, &rig), t!("compare-screen-changes", total = 1));
}

#[gpui_kit::test]
fn compare_large_asks_once_per_missing_window(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..300, (5_000, 5_000)));
    let before = rig.diff.line_requests().len();
    settle(cx, &rig.view);
    rig.view.update(cx, |_, cx| cx.notify());
    settle(cx, &rig.view);
    assert_eq!(
        rig.diff.line_requests().len(),
        before,
        "a shown window that arrived is not asked again"
    );
    rig.view.update(cx, |view, cx| {
        view.large_scroll_to(3_000);
        cx.notify();
    });
    settle(cx, &rig.view);
    let asked = rig.diff.line_requests();
    let last = asked.last().expect("a request after the scroll");
    assert!(last.contains(&3_000), "the new window is asked: {last:?}");
    assert!(last.end - last.start <= 2_000, "within one read: {last:?}");
    let focus = rig.diff.large_focus();
    assert!(
        focus
            .last()
            .is_some_and(|line| (2_500..3_500).contains(line)),
        "the section in view goes first: {focus:?}"
    );
    rig.view.update(cx, |_, cx| cx.notify());
    settle(cx, &rig.view);
    assert_eq!(
        rig.diff.line_requests().len(),
        asked.len(),
        "no repeat while in flight"
    );
}

#[gpui_kit::test]
fn compare_large_arriving_hunks_never_move_the_top_line(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..100, (1_000, 1_010)));
    push(
        cx,
        &rig,
        &sink,
        snapshot(vec![hunk(900..900, 900..910)], 1, 1),
    );
    rig.view.update(cx, |view, cx| {
        view.large_scroll_to(500);
        cx.notify();
    });
    settle(cx, &rig.view);
    let top = |cx: &mut TestAppContext| {
        let probe = rig
            .view
            .read_with(cx, |view, _| view.large_probe())
            .unwrap();
        (
            probe.top,
            rig.view
                .read_with(cx, |view, _| view.large_row(probe.top))
                .unwrap()
                .1,
        )
    };
    let (row, line) = top(cx);
    assert_eq!((row, line.map(|(at, _)| at)), (500, Some(500)));
    let above = vec![hunk(10..10, 10..20), hunk(900..900, 910..920)];
    push(cx, &rig, &sink, snapshot(above, 2, 2));
    let (row, line) = top(cx);
    assert_eq!(
        (row, line.map(|(at, _)| at)),
        (510, Some(500)),
        "ten rows arrived above; the same line stays on top"
    );
}

#[gpui_kit::test]
fn compare_large_next_past_known_hunks_looks_further_then_jumps(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..100, (1_000, 1_000)));
    push(cx, &rig, &sink, snapshot(vec![hunk(10..11, 10..11)], 2, 1));
    rig.press(cx, "alt-down");
    assert_eq!(
        status(cx, &rig),
        t!("compare-screen-change", current = 1, total = 1)
    );
    rig.press(cx, "alt-down");
    assert_eq!(
        status(cx, &rig),
        t!("compare-screen-looking-further", percent = 40)
    );
    let both = vec![hunk(10..11, 10..11), hunk(700..702, 700..702)];
    push(cx, &rig, &sink, snapshot(both, 3, 2));
    assert_eq!(
        status(cx, &rig),
        t!("compare-screen-change", current = 2, total = 2)
    );
    let probe = rig
        .view
        .read_with(cx, |view, _| view.large_probe())
        .unwrap();
    assert!(
        probe.top <= 700 && 700 < probe.top + 60,
        "jumped to it: {}",
        probe.top
    );
}

#[gpui_kit::test]
fn compare_large_esc_stops_looking_further_and_keeps_the_screen(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, lines(0..100, (1_000, 1_000)));
    push(cx, &rig, &sink, snapshot(vec![hunk(10..11, 10..11)], 2, 1));
    rig.press(cx, "alt-down");
    rig.press(cx, "alt-down");
    rig.press(cx, "escape");
    assert!(
        rig.view
            .read_with(cx, |view, _| view.file_screen().is_some())
    );
    let one = t!("compare-screen-change", current = 1, total = 1);
    assert_eq!(status(cx, &rig), one);
    let both = vec![hunk(10..11, 10..11), hunk(700..702, 700..702)];
    push(cx, &rig, &sink, snapshot(both, 3, 2));
    assert_eq!(
        status(cx, &rig),
        t!("compare-screen-change", current = 1, total = 2)
    );
}

#[gpui_kit::test]
fn compare_large_done_with_no_more_hunks_ends_the_wait(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, snapshot(vec![hunk(10..11, 10..11)], 2, 1));
    rig.press(cx, "alt-down");
    rig.press(cx, "alt-down");
    push(cx, &rig, &sink, snapshot(vec![hunk(10..11, 10..11)], 5, 2));
    assert_eq!(
        status(cx, &rig),
        t!("compare-screen-change", current = 1, total = 1)
    );
}

#[test]
fn compare_large_maps_unified_rows_and_unequal_gaps() {
    let hunks = [hunk(10..12, 10..13), hunk(50..51, 51..51)];
    let done = || Settled {
        done: true,
        evicted: Vec::new(),
    };
    let unified = RowMap::new(&hunks, Layout::Unified, Some((100, 99)), done());
    let at = |map: &RowMap, row| {
        let shown = map.at(row);
        (shown.kind, shown.left, shown.right)
    };
    assert_eq!(at(&unified, 11), (ShownKind::Deleted, Some(11), None));
    assert_eq!(at(&unified, 12), (ShownKind::Inserted, None, Some(10)));
    assert_eq!(at(&unified, 15), (ShownKind::Equal, Some(12), Some(13)));
    assert_eq!(unified.row_of(Anchor::Right(12)), 14);
    // After the second hunk 49 left lines face 48 right lines: no settled pairing, so the
    // rows are pending, paired one to one, and not a change to step to.
    let side = RowMap::new(&hunks, Layout::SideBySide, Some((100, 99)), done());
    assert_eq!(side.count(), 10 + 3 + 38 + 1 + 49);
    assert_eq!(at(&side, 52), (ShownKind::Pending, Some(51), Some(51)));
    assert_eq!(at(&side, 100), (ShownKind::Pending, Some(99), None));
    assert_eq!(side.at(52).hunk, None);
    assert_eq!(side.at(51).hunk, Some(1));
    assert_eq!(side.row_of(Anchor::Left(60)), 61);
    let open = RowMap::new(&hunks[..1], Layout::SideBySide, None, done());
    assert_eq!(
        at(&open, 5_000),
        (ShownKind::Pending, Some(4_999), Some(5_000))
    );
}

fn notes(cx: &mut TestAppContext, rig: &Rig) -> String {
    let probe = rig.view.read_with(cx, |view, _| view.large_probe());
    probe.expect("a large body").notes.to_string()
}

fn step(kind: LargeStep, reason: LargeStepReason, section: std::ops::Range<u64>) -> LargeStepView {
    LargeStepView {
        step: kind,
        lines: section.end - section.start,
        section,
        reason,
    }
}

/// A snapshot of request 1 with `steps`, sections `done` of 5.
fn with_steps(steps: Vec<LargeStepView>, done: u64) -> DiffUpdate {
    match snapshot(Vec::new(), done, 1) {
        DiffUpdate::Large {
            request,
            hunks,
            progress,
            ack,
            ..
        } => DiffUpdate::Large {
            request,
            hunks,
            steps,
            progress,
            ack,
        },
        other => other,
    }
}

#[gpui_kit::test]
fn compare_large_progress_line_runs_until_the_last_section(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    push(cx, &rig, &sink, snapshot(Vec::new(), 2, 1));
    assert_eq!(
        notes(cx, &rig),
        t!("compare-screen-comparing-percent", percent = 40)
    );
    push(cx, &rig, &sink, snapshot(Vec::new(), 5, 2));
    assert_eq!(notes(cx, &rig), "", "the line goes when the diff is done");
}

#[gpui_kit::test]
fn compare_large_step_notices_name_the_step(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    let steps = vec![
        step(LargeStep::Coarse, LargeStepReason::Budget, 0..90),
        step(LargeStep::Coarse, LargeStepReason::Budget, 90..180),
        step(LargeStep::Coarse, LargeStepReason::SectionCap, 180..270),
        step(LargeStep::PagedHunks, LargeStepReason::Paged, 270..360),
    ];
    push(cx, &rig, &sink, with_steps(steps, 5));
    let want = [
        t!("compare-screen-notice-coarse-budget", sections = 2),
        t!("compare-screen-notice-coarse-cap", sections = 1),
        t!("compare-screen-notice-paged"),
    ]
    .join(" · ");
    assert_eq!(notes(cx, &rig), want);
    push(cx, &rig, &sink, with_steps(Vec::new(), 5));
    assert_eq!(notes(cx, &rig), "", "no step, no notice");
}

/// Whether the large body shows the link to the memory budget setting.
fn has_link(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view
        .read_with(cx, |view, _| view.large_probe())
        .expect("a large body")
        .budget_link
}

#[gpui_kit::test]
fn compare_large_budget_link_opens_settings_at_the_memory_budget(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    let cap = step(LargeStep::Coarse, LargeStepReason::SectionCap, 0..90);
    push(cx, &rig, &sink, with_steps(vec![cap], 5));
    assert!(!has_link(cx, &rig), "a cap notice has no setting to raise");
    let budget = step(LargeStep::Coarse, LargeStepReason::Budget, 90..180);
    push(cx, &rig, &sink, with_steps(vec![budget], 5));
    assert!(has_link(cx, &rig));
    assert!(
        !notes(cx, &rig).contains("Settings"),
        "the link names the setting"
    );
    cx.update_window(rig.window, |_, window, cx| {
        window.click("compare-screen-budget-link", cx);
    })
    .expect("window");
    settle(cx, &rig.view);
    let (open, row) = rig
        .view
        .read_with(cx, |view, _| (view.settings.is_open(), view.settings_row()));
    assert!(open, "the link opens Settings");
    let want = crate::settings::FIELDS
        .iter()
        .position(|spec| spec.field == crate::settings::Field::CompareBudgetMib);
    assert_eq!(Some(row), want, "at the memory budget row");
}

#[gpui_kit::test]
fn compare_large_details_drawer_lists_kind_sizes_step_and_progress(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, sink) = open_large(cx);
    let coarse = step(LargeStep::Coarse, LargeStepReason::SectionCap, 0..90);
    push(cx, &rig, &sink, with_steps(vec![coarse], 2));
    let facts = |cx: &mut TestAppContext| rig.view.read_with(cx, |view, _| view.body_facts_probe());
    assert_eq!(facts(cx), None, "the drawer starts closed");
    rig.press(cx, "ctrl-i");
    settle(cx, &rig.view);
    let want = [
        "Kind: Large text",
        "Sizes: 1.0 GB ↔ 1.0 GB",
        "Engine step: Coarse blocks",
        "Progress: Comparing… 40 %",
    ];
    assert_eq!(facts(cx), Some(want.map(str::to_owned).to_vec()));
    rig.press(cx, "ctrl-i");
    settle(cx, &rig.view);
    assert_eq!(facts(cx), None, "Ctrl+I closes it again");
}

/// Opens a large diff with the budget notice and its link showing.
fn open_with_link(cx: &mut TestAppContext) -> Rig {
    let (rig, sink) = open_large(cx);
    let budget = step(LargeStep::Coarse, LargeStepReason::Budget, 90..180);
    push(cx, &rig, &sink, with_steps(vec![budget], 5));
    assert!(has_link(cx, &rig));
    rig
}

/// Whether the open compare file screen holds focus.
fn file_focused(cx: &mut TestAppContext, rig: &Rig) -> bool {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, _| view.file_screen_focused(window))
    })
    .expect("window")
}

#[gpui_kit::test]
fn compare_large_closing_settings_returns_focus_to_the_diff(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = open_with_link(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.click("compare-screen-budget-link", cx);
    })
    .expect("window");
    settle(cx, &rig.view);
    assert!(rig.view.read_with(cx, |view, _| view.settings.is_open()));
    rig.press(cx, "escape");
    settle(cx, &rig.view);
    assert!(!rig.view.read_with(cx, |view, _| view.settings.is_open()));
    assert!(file_focused(cx, &rig), "the diff has focus again");
    rig.press(cx, "ctrl-i");
    settle(cx, &rig.view);
    let facts = rig.view.read_with(cx, |view, _| view.body_facts_probe());
    assert!(facts.is_some(), "Ctrl+I opens the details drawer");
}

/// Moves focus along the window's tab order until the budget link has it; false if it never does.
fn tab_to_link(cx: &mut TestAppContext, rig: &Rig) -> bool {
    for _ in 0..16 {
        let on_link = rig
            .window
            .update(cx, |_, window, cx| {
                window.focus_next(cx);
                window.render_frame(cx);
                window.find("compare-screen-budget-link").focused() == Some(true)
            })
            .expect("window");
        if on_link {
            return true;
        }
    }
    false
}

#[gpui_kit::test]
fn compare_large_budget_link_works_from_the_keyboard(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = open_with_link(cx);
    let opened =
        |cx: &mut TestAppContext| rig.view.read_with(cx, |view, _| view.settings.is_open());
    assert!(tab_to_link(cx, &rig), "Tab reaches the link");
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    assert!(opened(cx), "Enter opens Settings");
    rig.press(cx, "escape");
    settle(cx, &rig.view);
    assert!(!opened(cx), "Escape closes Settings");
    assert!(tab_to_link(cx, &rig), "Tab reaches the link again");
    rig.press(cx, "space");
    settle(cx, &rig.view);
    assert!(opened(cx), "Space opens Settings");
}
