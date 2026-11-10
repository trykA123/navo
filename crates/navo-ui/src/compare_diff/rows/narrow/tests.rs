//! Tests for narrowing a card to the hunks a search matched.

use super::{Slot, slots};
use crate::compare_diff::hooks::{CardRow, DiffRow};

/// A card of `hunks` hunks: one context line and one changed line each, gaps between.
fn rows(hunks: u32) -> Vec<CardRow> {
    let line = |row: DiffRow, hunk: u32| CardRow::Line {
        row,
        hunk,
        left: None,
        right: None,
    };
    let mut out = Vec::new();
    for hunk in 0..hunks {
        if hunk > 0 {
            out.push(CardRow::Gap { lines: 4 });
        }
        out.push(line(DiffRow::Equal(hunk, hunk), hunk));
        out.push(line(DiffRow::Changed(hunk + 1, hunk + 1), hunk));
    }
    out
}

#[test]
fn without_a_search_every_row_stays() {
    let rows = rows(2);
    let all: Vec<Slot> = (0..rows.len()).map(Slot::Row).collect();
    assert_eq!(slots(&rows, None), all);
}

#[test]
fn skipped_hunks_fold_into_one_row_and_gaps_only_join_kept_hunks() {
    // Rows: h0 = 0,1; gap 2; h1 = 3,4; gap 5; h2 = 6,7; gap 8; h3 = 9,10; gap 11; h4 = 12,13.
    let rows = rows(5);
    let got = slots(&rows, Some(&[0, 3, 4]));
    assert_eq!(
        got,
        [
            Slot::Row(0),
            Slot::Row(1),
            Slot::Others(2),
            Slot::Row(9),
            Slot::Row(10),
            Slot::Row(11),
            Slot::Row(12),
            Slot::Row(13),
        ]
    );
}

#[test]
fn a_leading_run_and_a_trailing_run_each_get_a_row() {
    let rows = rows(4);
    let got = slots(&rows, Some(&[1]));
    assert_eq!(
        got,
        [Slot::Others(1), Slot::Row(3), Slot::Row(4), Slot::Others(2)]
    );
}

#[test]
fn the_more_row_stays_after_the_kept_hunks() {
    let mut rows = rows(2);
    rows.push(CardRow::More { hunks: 9 });
    let got = slots(&rows, Some(&[0]));
    assert_eq!(
        got,
        [Slot::Row(0), Slot::Row(1), Slot::Others(1), Slot::Row(5)]
    );
}
