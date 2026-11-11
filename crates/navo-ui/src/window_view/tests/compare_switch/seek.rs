//! Alt+V Text / Bytes (V41b fix round): the seek survives regions landing after the open, a gap
//! top row maps to a real line, the unified list maps through its real rows, the switch never
//! draws a blank body, and a forced bytes view clears when the file stops being binary.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::super::compare_hex::{frame, hex_rig, probe};
use super::super::compare_screen::Rig;
use super::super::compare_screen::body::pair;
use super::{LINE, labels, open};
use crate::commands::compare::{ViewBytes, ViewText};
use crate::compare_diff::{
    ByteRegionView, DiffRow, DiffUpdate, HexAlign, HunkView, Layout, PairKind, PairText,
};
use crate::window_view::compare_screen::menu::ShowUnified;

/// A pair of `len` lines whose lines each start `LINE` bytes apart.
fn lines(len: u32, changed: &[u32]) -> PairText {
    PairText {
        left_starts: (0..=u64::from(len)).map(|line| line * LINE).collect(),
        ..pair(len, changed)
    }
}

/// A left side of 1000 bytes against a right side of 3000: Content alignment, the default.
fn unequal(cx: &mut TestAppContext) -> Rig {
    hex_rig(cx, vec![0x11; 1000], vec![0x22; 3000])
}

/// Sends the one region `2000` bytes inserted at left offset 100 for pair `request`.
fn insertion(cx: &mut TestAppContext, rig: &Rig, request: u64) {
    let run = probe(cx, rig).run;
    let sink = rig.diff.pair_sink();
    let regions = vec![ByteRegionView {
        left: 100..100,
        right: 100..2100,
        changed: 2000,
    }];
    let bytes = DiffUpdate::Bytes {
        request,
        align: HexAlign::Content,
        run,
        regions,
        progress: None,
    };
    rig.push(cx, &sink, bytes);
    frame(cx, rig);
}

/// Opens a 60-line text with the change on line 40, scrolled by Alt+Down; returns the rig and
/// the line on top, which is past the insertion at byte 100.
fn text_past_the_insertion(cx: &mut TestAppContext) -> (Rig, usize) {
    let rig = unequal(cx);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Text), Some(lines(60, &[40])));
    rig.press(cx, "alt-down");
    let (_, built, _) = rig.probe(cx, Layout::SideBySide);
    assert!(
        built[0] * LINE as usize > 130,
        "top is past byte 100: {built:?}"
    );
    (rig, built[0])
}

#[gpui_kit::test]
fn compare_switch_bytes_keeps_its_offset_when_the_content_rows_arrive_late(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, top) = text_past_the_insertion(cx);
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    insertion(cx, &rig, 2);
    let want = top as u64 * LINE;
    let shown = probe(cx, &rig);
    assert_eq!(shown.align, "content");
    let at = shown
        .top_left
        .expect("the top row holds left bytes, not a gap");
    assert!(
        at <= want && want < at + 16,
        "row at {at} holds byte {want}: {shown:?}"
    );
}

#[gpui_kit::test]
fn compare_switch_text_from_a_gap_top_row_opens_at_the_next_left_line(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _) = text_past_the_insertion(cx);
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    insertion(cx, &rig, 2);
    let top = rig.view.update(cx, |view, _| view.file_hex_wheel(-30.));
    assert!(top.is_some_and(|row| row > 20 && row < 131));
    assert_eq!(probe(cx, &rig).top_left, None, "the top row is in the gap");
    rig.dispatch(cx, &ViewText);
    open(cx, &rig, (3, PairKind::Text), Some(lines(60, &[40])));
    let (_, built, _) = rig.probe(cx, Layout::SideBySide);
    assert_eq!(built[0], 6, "byte 100 is on line 6, not line 0: {built:?}");
}

/// A run of 100 changed rows, then a deleted row, a changed row and 60 equal rows: the unified
/// list shows all lefts of the run first, then all its rights.
fn long_run() -> PairText {
    let mut rows: Vec<DiffRow> = (0..100).map(|at| DiffRow::Changed(at, at)).collect();
    rows.push(DiffRow::Deleted(100));
    rows.push(DiffRow::Changed(101, 100));
    rows.extend((0..60).map(|at| DiffRow::Equal(102 + at, 101 + at)));
    let lines = vec![String::new(); 162];
    PairText {
        left_starts: (0..=162).map(|line| line * LINE).collect(),
        left: lines.clone(),
        right: lines,
        hunks: vec![HunkView {
            rows: 0..102,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 101,
            removed: 102,
        }],
        rows,
        ..pair(1, &[])
    }
}

#[gpui_kit::test]
fn compare_switch_unified_round_trip_lands_on_the_same_line(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x2000], vec![0x11; 0x2000]);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Text), Some(long_run()));
    rig.dispatch(cx, &ShowUnified);
    rig.view
        .update(cx, |view, _| view.file_seek_offset(60 * LINE));
    frame(cx, &rig);
    let (_, built, _) = rig.probe(cx, Layout::Unified);
    let top = built[0];
    assert!(
        (50..100).contains(&top),
        "the sought line is on top, inside the run: {built:?}"
    );
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(
        probe(cx, &rig).built.start as u64,
        top as u64,
        "row n holds byte 16 * n"
    );
    rig.dispatch(cx, &ViewText);
    open(cx, &rig, (3, PairKind::Text), Some(long_run()));
    let (_, built, _) = rig.probe(cx, Layout::Unified);
    assert_eq!(built[0], top, "the same line is on top again: {built:?}");
}

/// Whether the last frame drew the text list and whether it drew hex rows.
fn body_drawn(cx: &mut TestAppContext, rig: &Rig) -> (bool, bool) {
    let first = rig
        .view
        .update(cx, |view, _| view.file_hex_probe())
        .map_or(0, |hex| hex.built.start);
    cx.update_window(rig.window, |_, window, _| {
        // Hex row `n` is drawn as side slots `2n` (left) and `2n + 1` (right).
        (
            window.try_find("compare-file-rows").is_some(),
            window.try_find(("paged-row", first * 2)).is_some(),
        )
    })
    .expect("window")
}

#[gpui_kit::test]
fn compare_switch_bytes_never_draws_a_blank_body_before_the_first_hex_rows(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _) = text_past_the_insertion(cx);
    assert!(
        body_drawn(cx, &rig).0,
        "the text is drawn before the switch"
    );
    rig.dispatch(cx, &ViewBytes);
    let sink = rig.diff.pair_sink();
    let opened = DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Binary,
        text: None,
    };
    sink(opened);
    rig.view.update(cx, |view, cx| view.wake(cx));
    for _ in 0..40 {
        cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        let (text, hex) = body_drawn(cx, &rig);
        let shown = probe(cx, &rig);
        assert!(text || hex, "blank frame: {shown:?}");
        if shown.rows > 0 {
            // The rows are installed: a drawn body without bytes has no `paged-row` cells.
            assert!(hex, "rows installed but the body draws no bytes: {shown:?}");
            assert!(
                shown.left_hex.iter().any(Option::is_some),
                "no bytes: {shown:?}"
            );
            return;
        }
        crate::window_view::tests::settle(cx, &rig.view);
    }
    panic!("the hex rows never landed");
}

/// Sends one more region batch for pair `request`: `100` bytes inserted at left offset 600.
fn second_batch(cx: &mut TestAppContext, rig: &Rig, request: u64) {
    let run = probe(cx, rig).run;
    let sink = rig.diff.pair_sink();
    let bytes = DiffUpdate::Bytes {
        request,
        align: HexAlign::Content,
        run,
        regions: vec![ByteRegionView {
            left: 600..600,
            right: 2700..2800,
            changed: 100,
        }],
        progress: None,
    };
    rig.push(cx, &sink, bytes);
    frame(cx, rig);
}

#[gpui_kit::test]
fn compare_hex_a_gap_top_row_keeps_its_right_offset_when_more_regions_land(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = unequal(cx);
    rig.press(cx, "enter");
    open(cx, &rig, (1, PairKind::Binary), None);
    insertion(cx, &rig, 1);
    let top = rig.view.update(cx, |view, _| view.file_hex_wheel(30.));
    assert!(top.is_some_and(|row| row > 20 && row < 131), "top: {top:?}");
    let before = probe(cx, &rig);
    let want = before.top_right.expect("the gap row holds right bytes");
    assert_eq!(before.top_left, None, "the top row is in the gap");
    second_batch(cx, &rig, 1);
    let after = probe(cx, &rig);
    let at = after.top_right.expect("still a right-only row on top");
    assert!(
        at <= want && want < at + 16,
        "top moved from right byte {want} to {at}: {after:?}"
    );
}

#[gpui_kit::test]
fn compare_switch_a_reload_that_lands_large_clears_the_forced_bytes(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, _) = text_past_the_insertion(cx);
    rig.dispatch(cx, &ViewBytes);
    open(cx, &rig, (2, PairKind::Binary), None);
    assert_eq!(labels(cx, &rig), vec!["Show text".to_owned()]);
    open(
        cx,
        &rig,
        (
            3,
            PairKind::Large {
                left: 1 << 30,
                right: 1 << 30,
            },
        ),
        None,
    );
    assert_eq!(
        rig.diff.views(),
        vec![(2, true), (4, false)],
        "the flag is cleared"
    );
    open(cx, &rig, (4, PairKind::Text), Some(lines(60, &[40])));
    assert_eq!(
        labels(cx, &rig),
        vec!["Show bytes".to_owned()],
        "text offers bytes again"
    );
}
