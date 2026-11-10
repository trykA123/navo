//! Tests for card planning: context merging, clipping and the row cap.

use std::ops::Range;

use proptest::prelude::*;

use super::*;
use crate::text::digest::HunkDigests;

fn equal(left: usize, right: usize) -> Row {
    Row::Equal { left, right }
}

fn changed(left: usize, right: usize) -> Row {
    Row::Changed { left, right }
}

fn deleted(left: usize) -> Row {
    Row::Deleted { left }
}

fn inserted(right: usize) -> Row {
    Row::Inserted { right }
}

fn hunk(left: Range<usize>, right: Range<usize>) -> Hunk {
    Hunk {
        left_lines: left,
        right_lines: right,
        left_bytes: 0..0,
        right_bytes: 0..0,
    }
}

fn alignment(rows: Vec<Row>, hunks: Vec<Hunk>) -> Alignment {
    Alignment {
        rows,
        hunks,
        digests: HunkDigests::default(),
        raw_equal: false,
        display_equal: false,
        fallback: None,
    }
}

fn index(row: &CardRow) -> Option<usize> {
    match row {
        CardRow::Row { index, .. } => Some(*index),
        CardRow::Gap { .. } => None,
    }
}

#[test]
fn no_hunks_gives_no_rows() {
    let rows = (0..4).map(|at| equal(at, at)).collect();
    let plan = card(&alignment(rows, Vec::new()), CardLimits::default());
    assert_eq!(plan, CardPlan::default());
}

#[test]
fn nearby_hunks_share_their_context_without_a_gap() {
    let rows = vec![
        equal(0, 0),
        changed(1, 1),
        equal(2, 2),
        equal(3, 3),
        changed(4, 4),
        equal(5, 5),
    ];
    let hunks = vec![hunk(1..2, 1..2), hunk(4..5, 4..5)];
    let limits = CardLimits {
        context: 3,
        max_rows: 2_000,
    };
    let plan = card(&alignment(rows, hunks), limits);
    assert_eq!(plan.shown_hunks, 2);
    assert_eq!(plan.hunks, vec![1..2, 4..5]);
    assert_eq!(plan.rows.len(), 6);
    assert!(
        !plan
            .rows
            .iter()
            .any(|row| matches!(row, CardRow::Gap { .. }))
    );
    assert_eq!(
        plan.rows.iter().filter_map(index).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
}

#[test]
fn context_is_clipped_at_the_file_start() {
    let rows = vec![equal(0, 0), changed(1, 1), equal(2, 2), equal(3, 3)];
    let hunks = vec![hunk(1..2, 1..2)];
    let limits = CardLimits {
        context: 3,
        max_rows: 2_000,
    };
    let plan = card(&alignment(rows, hunks), limits);
    assert_eq!(
        plan.rows.iter().filter_map(index).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert!(
        !plan
            .rows
            .iter()
            .any(|row| matches!(row, CardRow::Gap { .. }))
    );
}

#[test]
fn context_is_clipped_at_the_file_end() {
    let rows = vec![
        changed(0, 0),
        equal(1, 1),
        equal(2, 2),
        equal(3, 3),
        equal(4, 4),
        equal(5, 5),
    ];
    let hunks = vec![hunk(0..1, 0..1)];
    let limits = CardLimits {
        context: 3,
        max_rows: 2_000,
    };
    let plan = card(&alignment(rows, hunks), limits);
    assert_eq!(
        plan.rows.iter().filter_map(index).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert!(
        !plan
            .rows
            .iter()
            .any(|row| matches!(row, CardRow::Gap { .. }))
    );
}

#[test]
fn a_distant_hunk_gets_one_gap_row() {
    let mut rows = vec![changed(0, 0)];
    rows.extend((1..6).map(|at| equal(at, at)));
    rows.push(changed(6, 6));
    let hunks = vec![hunk(0..1, 0..1), hunk(6..7, 6..7)];
    let limits = CardLimits {
        context: 1,
        max_rows: 2_000,
    };
    let plan = card(&alignment(rows, hunks), limits);
    assert_eq!(plan.rows[0], CardRow::Row { index: 0, hunk: 0 });
    assert_eq!(plan.rows[1], CardRow::Row { index: 1, hunk: 0 });
    assert_eq!(plan.rows[2], CardRow::Gap { lines: 3 });
    assert_eq!(plan.rows[3], CardRow::Row { index: 5, hunk: 1 });
    assert_eq!(plan.rows[4], CardRow::Row { index: 6, hunk: 1 });
    assert_eq!(plan.rows.len(), 5);
}

#[test]
fn the_cap_counts_more_hunks() {
    let mut rows = vec![changed(0, 0)];
    rows.extend((1..6).map(|at| equal(at, at)));
    rows.push(changed(6, 6));
    rows.extend((7..12).map(|at| equal(at, at)));
    rows.push(changed(12, 12));
    let hunks = vec![hunk(0..1, 0..1), hunk(6..7, 6..7), hunk(12..13, 12..13)];
    let plan = card(
        &alignment(rows, hunks),
        CardLimits {
            context: 0,
            max_rows: 3,
        },
    );
    assert_eq!(plan.shown_hunks, 2);
    assert_eq!(plan.more_hunks, 1);
    assert_eq!(plan.hunks, vec![0..1, 2..3]);
    assert_eq!(plan.rows.len(), 3);
    assert_eq!(
        plan.rows.iter().filter_map(index).collect::<Vec<_>>(),
        vec![0, 6]
    );
}

#[test]
fn the_cap_never_cuts_a_hunk_in_half() {
    let rows = vec![
        changed(0, 0),
        changed(1, 1),
        equal(2, 2),
        changed(3, 3),
        changed(4, 4),
    ];
    let hunks = vec![hunk(0..2, 0..2), hunk(3..5, 3..5)];
    let plan = card(
        &alignment(rows, hunks),
        CardLimits {
            context: 0,
            max_rows: 3,
        },
    );
    assert_eq!(plan.shown_hunks, 1);
    assert_eq!(plan.more_hunks, 1);
    assert_eq!(plan.hunks, vec![0..2]);
    assert_eq!(
        plan.rows.iter().filter_map(index).collect::<Vec<_>>(),
        vec![0, 1]
    );
}

fn generated_alignment(spec: &[(usize, usize, usize)]) -> Alignment {
    let mut rows = Vec::new();
    let mut hunks = Vec::new();
    let (mut left, mut right) = (0, 0);
    for &(kind, size, gap) in spec {
        for _ in 0..gap {
            rows.push(equal(left, right));
            left += 1;
            right += 1;
        }
        let (left_start, right_start) = (left, right);
        match kind {
            0 => {
                rows.extend((0..size).map(|at| changed(left + at, right + at)));
                left += size;
                right += size;
            }
            1 => {
                rows.extend((0..size).map(|at| deleted(left + at)));
                left += size;
            }
            _ => {
                rows.extend((0..size).map(|at| inserted(right + at)));
                right += size;
            }
        }
        hunks.push(hunk(left_start..left, right_start..right));
    }
    alignment(rows, hunks)
}

fn spec_strategy() -> impl Strategy<Value = Vec<(usize, usize, usize)>> {
    prop::collection::vec((0usize..3, 1usize..6, 0usize..8), 0..8)
}

proptest! {
    #[test]
    fn shown_hunks_are_complete_and_ordered(
        spec in spec_strategy(),
        context in 0usize..5,
        max_rows in 0usize..40,
    ) {
        let alignment = generated_alignment(&spec);
        let limits = CardLimits { context, max_rows };
        let plan = card(&alignment, limits);
        prop_assert_eq!(plan.shown_hunks, plan.hunks.len());
        prop_assert_eq!(plan.shown_hunks + plan.more_hunks, alignment.hunks.len());
        prop_assert!(plan.rows.len() <= max_rows);
        let abs = hunk_row_ranges(&alignment);
        let mut previous = None;
        for row in &plan.rows {
            match *row {
                CardRow::Row { index, .. } => {
                    if let Some(last) = previous {
                        prop_assert!(index > last);
                    }
                    previous = Some(index);
                }
                CardRow::Gap { lines } => prop_assert!(lines > 0),
            }
        }
        for (hunk_index, card_range) in plan.hunks.iter().enumerate() {
            let own = &abs[hunk_index];
            prop_assert_eq!(card_range.len(), own.len());
            for (offset, absolute) in own.clone().enumerate() {
                prop_assert_eq!(
                    plan.rows[card_range.start + offset],
                    CardRow::Row { index: absolute, hunk: hunk_index }
                );
            }
        }
    }
}
