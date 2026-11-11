//! Picked lines (V32): Shift+↑/↓, Space and the tick boxes pick lines, Ctrl+Alt+←/→ copy only
//! those, Ctrl+Shift+Del removes one-sided changes, and Esc clears the pick first.

use std::collections::BTreeSet;

use gpui_kit::TestAppContext;
use navo_l10n::tr;

use super::{DIGEST, text};
use crate::compare_diff::{CopyDirection, CopyResult, DiffRow, HunkView, PairText};
use crate::window_view::tests::compare_screen::{Rig, rig};

/// Equal rows, then change 0 (rows 2..5: two changed, one left-only), change 1 (row 8, right
/// only) and change 2 (row 10, left only). Left lines 2, 3, 4 and 9, right lines 2, 3 and 7
/// can be picked.
const OPS: &str = "EECCDEEEIEDEEEEEEEEE";

/// A change over `rows` with a distinct byte range and a digest.
fn change(rows: std::ops::Range<u32>) -> HunkView {
    let at = u64::from(rows.start);
    HunkView {
        rows,
        left_bytes: at * 10..at * 10 + 4,
        right_bytes: at * 10..at * 10 + 5,
        digest: Some(DIGEST),
        added: 1,
        removed: 1,
    }
}

/// The pair described by `OPS`.
fn pick_text() -> PairText {
    let (mut l, mut r) = (0, 0);
    let mut rows = Vec::new();
    for op in OPS.chars() {
        rows.push(match op {
            'E' => DiffRow::Equal(l, r),
            'C' => DiffRow::Changed(l, r),
            'D' => DiffRow::Deleted(l),
            _ => DiffRow::Inserted(r),
        });
        l += u32::from(op != 'I');
        r += u32::from(op != 'D');
    }
    PairText {
        left_starts: Vec::new(),
        left: vec!["line".to_owned(); l as usize],
        right: vec!["line".to_owned(); r as usize],
        rows,
        hunks: vec![change(2..5), change(8..9), change(10..11)],
        ..text()
    }
}

/// A set of line numbers, as the copy carries them.
fn lines(of: &[usize]) -> Option<BTreeSet<usize>> {
    Some(of.iter().copied().collect())
}

/// A rig with the pick pair open and drawn.
fn opened(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    rig.open_with(cx, pick_text());
    rig
}

#[gpui_kit::test]
fn compare_copy_pick_shift_down_picks_lines_and_the_copy_sends_only_them(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "shift-down");
    rig.press(cx, "shift-down");
    rig.press(cx, "ctrl-alt-right");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].lines, lines(&[2, 3]));
    assert_eq!(asked[0].direction, CopyDirection::ToRight);
    assert_eq!(asked[0].digest, DIGEST);
}

#[gpui_kit::test]
fn compare_copy_pick_shift_up_shrinks_the_range(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    for _ in 0..3 {
        rig.press(cx, "shift-down");
    }
    rig.press(cx, "shift-up");
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.diff.copies()[0].lines, lines(&[2, 3]));
}

#[gpui_kit::test]
fn compare_copy_pick_lines_across_changes_go_out_as_one_copy(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    for _ in 0..4 {
        rig.press(cx, "shift-down");
    }
    rig.press(cx, "ctrl-alt-right");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 2, "one request per change, in one call");
    assert_eq!(asked[0].lines, lines(&[2, 3, 4]));
    assert_eq!(asked[1].lines, lines(&[9]));
    assert_eq!(asked[1].left_bytes, 100..104);
}

#[gpui_kit::test]
fn compare_copy_pick_space_ticks_and_unticks_one_line(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "space");
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.diff.copies()[0].lines, lines(&[2]));
    rig.finish(cx, CopyDirection::ToRight, CopyResult::Copied);
    rig.open_with(cx, pick_text());
    rig.press(cx, "space");
    rig.press(cx, "space");
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.diff.copies().len(), 1, "an unticked line sends nothing");
    assert_eq!(rig.note(cx), Some((tr("compare-copy-pick"), false, true)));
}

#[gpui_kit::test]
fn compare_copy_pick_the_wrong_side_says_so_and_copies_nothing(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "shift-down");
    rig.press(cx, "ctrl-alt-left");
    assert!(rig.diff.copies().is_empty());
    let said = tr("compare-copy-pick-side");
    assert_eq!(rig.note(cx), Some((said, false, true)));
    rig.press(cx, "ctrl-alt-right");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1, "the pick survived the wrong direction");
    assert_eq!(asked[0].lines, lines(&[2]));
}

#[gpui_kit::test]
fn compare_copy_pick_esc_clears_the_pick_before_it_leaves(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "shift-down");
    rig.press(cx, "escape");
    assert!(
        rig.has(cx, ("compare-file-row", 2)),
        "still in the file diff"
    );
    rig.press(cx, "ctrl-alt-right");
    assert!(rig.diff.copies().is_empty(), "the pick is gone");
    rig.press(cx, "escape");
    assert!(!rig.has(cx, ("compare-file-row", 2)), "the next Esc leaves");
}

#[gpui_kit::test]
fn compare_copy_pick_tick_boxes_show_on_hover_and_a_click_picks_the_line(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    assert!(
        !rig.has(cx, ("compare-tick-left", 3)),
        "hidden until hovered"
    );
    rig.hover(cx, ("compare-file-row", 3));
    assert!(rig.has(cx, ("compare-tick-left", 3)));
    assert!(rig.has(cx, ("compare-tick-right", 3)));
    assert!(
        !rig.has(cx, ("compare-tick-left", 0)),
        "no box on an equal line"
    );
    rig.click(cx, ("compare-tick-left", 3));
    assert!(
        rig.has(cx, ("compare-tick-left", 2)),
        "boxes stay while a line is picked"
    );
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.diff.copies()[0].lines, lines(&[3]));
}

#[gpui_kit::test]
fn compare_copy_pick_the_hover_bar_still_copies_the_whole_change(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.press(cx, "shift-down");
    rig.hover(cx, ("compare-file-row", 2));
    rig.click(cx, ("compare-copy-right", 2));
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].lines, None);
}

#[gpui_kit::test]
fn compare_copy_pick_remove_copies_a_one_sided_change_from_the_empty_side(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    rig.hover(cx, ("compare-file-row", 10));
    rig.click(cx, ("compare-tick-left", 10));
    rig.press(cx, "ctrl-shift-delete");
    let asked = rig.diff.copies();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].direction, CopyDirection::ToLeft);
    assert_eq!(
        (asked[0].lines.clone(), &asked[0].left_bytes),
        (None, &(100..104))
    );
    rig.finish(cx, CopyDirection::ToLeft, CopyResult::Copied);
    let said = tr("compare-removed-left");
    assert_eq!(rig.note(cx), Some((said, true, false)));
}

#[gpui_kit::test]
fn compare_copy_pick_remove_refuses_a_change_with_lines_on_both_sides(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = opened(cx);
    for _ in 0..3 {
        rig.press(cx, "shift-down");
    }
    rig.press(cx, "ctrl-shift-delete");
    assert!(rig.diff.copies().is_empty());
    let said = tr("compare-remove-whole");
    assert_eq!(rig.note(cx), Some((said, false, true)));
}
