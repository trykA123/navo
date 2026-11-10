//! Tests for the pick state: Shift+click ranges, one side at a time, and the removal rule.

use super::{Pick, PickSide};
use crate::compare_diff::{DiffRow, HunkView, PairFacts, PairRequest, PairText, ViewedKey};

/// Rows 1..6 are one change of five changed lines, each with a digest.
fn text() -> PairText {
    let rows: Vec<DiffRow> = (0..7)
        .map(|at| match at {
            1..=5 => DiffRow::Changed(at, at),
            _ => DiffRow::Equal(at, at),
        })
        .collect();
    PairText {
        left: vec![String::new(); 7],
        right: vec![String::new(); 7],
        rows,
        hunks: vec![HunkView {
            rows: 1..6,
            left_bytes: 0..1,
            right_bytes: 0..1,
            digest: Some([1; 16]),
            added: 5,
            removed: 5,
        }],
        facts: PairFacts {
            left_encoding: "",
            right_encoding: "",
            left_endings: "",
            right_endings: "",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    }
}

/// The picked lines, in order.
fn picked(pick: &Pick) -> Vec<u32> {
    pick.lines.iter().copied().collect()
}

#[test]
fn compare_copy_pick_shift_click_picks_the_range_from_the_last_click() {
    let (text, mut pick) = (text(), Pick::default());
    pick.click(&text, (PickSide::Left, 2), false);
    pick.click(&text, (PickSide::Left, 4), true);
    assert_eq!(picked(&pick), [2, 3, 4]);
    pick.click(&text, (PickSide::Left, 3), true);
    assert_eq!(picked(&pick), [2, 3], "the range follows the last click");
}

#[test]
fn compare_copy_pick_a_click_on_the_other_side_starts_over() {
    let (text, mut pick) = (text(), Pick::default());
    pick.click(&text, (PickSide::Left, 2), false);
    pick.click(&text, (PickSide::Right, 4), false);
    assert_eq!(picked(&pick), [4]);
}

#[test]
fn compare_copy_pick_a_click_off_a_changed_line_picks_nothing() {
    let (text, mut pick) = (text(), Pick::default());
    assert_eq!(pick.click(&text, (PickSide::Left, 0), false), None);
    assert!(!pick.has_lines());
}

#[test]
fn compare_copy_pick_removal_needs_every_line_of_a_one_sided_change() {
    let (text, mut pick) = (text(), Pick::default());
    pick.click(&text, (PickSide::Left, 2), false);
    let pair = PairRequest {
        id: 1,
        path: String::new(),
        left: None,
        right: None,
    };
    assert_eq!(pick.removal(&text, &pair), Err("compare-remove-whole"));
}
