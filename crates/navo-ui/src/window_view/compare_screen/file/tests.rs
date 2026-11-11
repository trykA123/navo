//! Tests for the file diff's row shapes.

use super::Shape;
use crate::compare_diff::{DiffRow, HunkView, Layout, PairFacts, PairText, ViewedKey};

/// Hunk `rows` with no bytes or digest.
fn hunk(rows: std::ops::Range<u32>) -> HunkView {
    HunkView {
        rows,
        left_bytes: 0..0,
        right_bytes: 0..0,
        digest: None,
        added: 0,
        removed: 0,
    }
}

#[test]
fn compare_screen_unified_hunks_start_after_the_rows_changed_before_them() {
    let rows = vec![
        DiffRow::Equal(0, 0),
        DiffRow::Changed(1, 1),
        DiffRow::Changed(2, 2),
        DiffRow::Equal(3, 3),
        DiffRow::Deleted(4),
        DiffRow::Changed(5, 4),
    ];
    let text = PairText {
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        left: vec![String::new(); 6],
        right: vec![String::new(); 5],
        rows,
        hunks: vec![hunk(1..3), hunk(4..6)],
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
    };
    let side = Shape::new(&text, Layout::SideBySide);
    assert_eq!((side.count, side.starts), (6, vec![1, 4]));
    let unified = Shape::new(&text, Layout::Unified);
    assert_eq!((unified.count, unified.starts), (9, vec![1, 6]));
}
