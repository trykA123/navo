use std::collections::BTreeSet;

use super::{
    Case, Side, apply_to, apply_whole, bom_lines, facts_of, hashed, laid, lines, picks, row_sets,
    utf16le, view, windows_view,
};
use crate::text::diff::{Fallback, Hunk, Row, diff_lines};
use crate::text::normalise::Options;
use crate::text::side::LineEnding;
use crate::text::{ApplyError, Direction, Picked, PickedHunk, TargetFacts, apply_picked};

macro_rules! run_picked {
    ($left:expr, $right:expr, $alignment:expr, $picks:expr, $direction:expr) => {{
        let rows = super::row_sets(&$alignment);
        let case = super::Case {
            left: &$left,
            right: &$right,
            alignment: &$alignment,
            rows: &rows,
            picks: &$picks,
            direction: $direction,
        };
        super::picked(&case)
    }};
}

#[test]
fn one_picked_line_of_a_five_line_hunk_changes_only_that_line() {
    let left = lines(&[
        ("Alpha", LineEnding::Lf),
        ("Bravo", LineEnding::Lf),
        ("Charlie", LineEnding::Lf),
        ("Delta", LineEnding::Lf),
        ("Echo", LineEnding::Lf),
    ]);
    let right = lines(&[
        ("alpha", LineEnding::Lf),
        ("bravo", LineEnding::CrLf),
        ("charlie", LineEnding::CrLf),
        ("delta", LineEnding::CrLf),
        ("echo", LineEnding::CrLf),
    ]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let out = run_picked!(
        &left,
        &right,
        &alignment,
        picks([2]),
        Direction::LeftToRight
    );
    assert_eq!(
        out,
        lines(&[
            ("alpha", LineEnding::Lf),
            ("bravo", LineEnding::CrLf),
            ("Charlie", LineEnding::CrLf),
            ("delta", LineEnding::CrLf),
            ("echo", LineEnding::CrLf),
        ])
        .bytes
    );
}

#[test]
fn picks_across_two_hunks_both_apply() {
    let left = lines(&[
        ("line ONE", LineEnding::Lf),
        ("keep", LineEnding::Lf),
        ("line TWO", LineEnding::Lf),
    ]);
    let right = lines(&[
        ("line one", LineEnding::Lf),
        ("keep", LineEnding::Lf),
        ("line two", LineEnding::Lf),
    ]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    assert_eq!(alignment.hunks.len(), 2);
    let out = run_picked!(
        &left,
        &right,
        &alignment,
        picks([0, 2]),
        Direction::LeftToRight
    );
    assert_eq!(out, left.bytes);
}

#[test]
fn a_picked_insert_only_line_lands_at_its_aligned_place() {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let right = lines(&[("one", LineEnding::Lf), ("three", LineEnding::Lf)]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let out = run_picked!(
        &left,
        &right,
        &alignment,
        picks([1]),
        Direction::LeftToRight
    );
    assert_eq!(out, left.bytes);
}

#[test]
fn a_picked_right_only_line_lands_at_its_aligned_place() {
    let left = lines(&[("one", LineEnding::Lf), ("three", LineEnding::Lf)]);
    let right = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let out = run_picked!(
        &left,
        &right,
        &alignment,
        picks([1]),
        Direction::RightToLeft
    );
    assert_eq!(out, right.bytes);
}

#[test]
fn picking_every_source_line_equals_the_whole_hunk() {
    let left = lines(&[("same", LineEnding::Lf), ("same", LineEnding::Lf)]);
    let right = lines(&[("samex", LineEnding::Lf), ("samex", LineEnding::Lf)]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let hunk = super::CopyHunk {
        hunk: alignment.hunks[0].clone(),
        digests: alignment.digests.clone(),
    };
    let all: BTreeSet<usize> = (0..left.lines.len()).collect();
    let picked = run_picked!(&left, &right, &alignment, all, Direction::LeftToRight);
    let whole = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(picked, whole);
}

#[test]
fn an_empty_pick_set_equals_the_whole_hunk_apply() {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let right = lines(&[("one", LineEnding::Lf), ("three", LineEnding::Lf)]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let hunk = super::CopyHunk {
        hunk: alignment.hunks[0].clone(),
        digests: alignment.digests.clone(),
    };
    let empty = run_picked!(
        &left,
        &right,
        &alignment,
        BTreeSet::new(),
        Direction::LeftToRight
    );
    let whole = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(empty, whole);
}

#[test]
fn token_cap_fallback_rows_are_not_copyable() {
    const COUNT: usize = 500_001;
    let left_lines: Vec<(String, LineEnding)> = (0..COUNT)
        .map(|_| ("a".to_owned(), LineEnding::Lf))
        .collect();
    let right_lines: Vec<(String, LineEnding)> = (0..COUNT - 1)
        .map(|_| ("b".to_owned(), LineEnding::Lf))
        .collect();
    let left = Side::with_bom(left_lines);
    let right = Side::new(right_lines);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    assert_eq!(alignment.fallback, Some(Fallback::TokenCap));
    assert_eq!(alignment.hunks.len(), 1);
    let picked = BTreeSet::from([0, 100_000, 200_000, 300_000, 400_000, 500_000]);
    let rows = row_sets(&alignment);
    let entries: Vec<PickedHunk<'_>> = alignment
        .hunks
        .iter()
        .zip(&rows)
        .map(|(hunk, rows)| PickedHunk {
            hunk,
            rows: rows.as_slice(),
        })
        .collect();
    let mut out = Vec::new();
    let result = apply_picked(
        &view(&right.bytes),
        &laid(&left.bytes),
        Picked {
            facts: facts_of(&right.lines),
            hunks: &entries,
            digests: &alignment.digests,
            left_starts: &left.starts,
            right_starts: &right.starts,
            lines: &picked,
            direction: Direction::LeftToRight,
        },
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::NotCopyable)), "{result:?}");
    assert!(out.is_empty());
}

#[test]
fn a_utf8_bom_source_is_skipped_for_picked_lines() {
    let source = bom_lines(&[("ONE", LineEnding::Lf), ("two", LineEnding::Lf)]);
    let target = lines(&[("one", LineEnding::Lf), ("two", LineEnding::Lf)]);
    let mut left_starts = source.starts.clone();
    left_starts[0] = 0;
    let hunk = hashed(
        Hunk {
            left_lines: 0..2,
            left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
            right_lines: 0..2,
            right_bytes: 0..u64::try_from(target.bytes.len()).unwrap(),
        },
        &source.bytes,
        &target.bytes,
    );
    let rows = [
        Row::Changed { left: 0, right: 0 },
        Row::Changed { left: 1, right: 1 },
    ];
    let picked = picks([0]);
    let entries = [PickedHunk {
        hunk: &hunk,
        rows: &rows,
    }];
    let mut out = Vec::new();
    apply_picked(
        &view(&target.bytes),
        &laid(&source.bytes),
        Picked {
            facts: facts_of(&target.lines),
            hunks: &entries,
            digests: &hunk.digests,
            left_starts: &left_starts,
            right_starts: &target.starts,
            lines: &picked,
            direction: Direction::LeftToRight,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        lines(&[("ONE", LineEnding::Lf), ("two", LineEnding::Lf)]).bytes
    );
}

#[test]
fn a_character_the_target_cannot_hold_is_an_error() {
    let target = b"x\n";
    let source = lines(&[("\u{1F600}", LineEnding::Lf)]);
    let hunk = hashed(
        Hunk {
            left_lines: 0..1,
            left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
            right_lines: 0..1,
            right_bytes: 0..2,
        },
        &source.bytes,
        target,
    );
    let rows = [Row::Changed { left: 0, right: 0 }];
    let picked = picks([0]);
    let entries = [PickedHunk {
        hunk: &hunk,
        rows: &rows,
    }];
    let mut out = Vec::new();
    let result = apply_picked(
        &windows_view(target),
        &view(&source.bytes),
        Picked {
            facts: TargetFacts {
                dominant: LineEnding::Lf,
                final_newline: true,
            },
            hunks: &entries,
            digests: &hunk.digests,
            left_starts: &source.starts,
            right_starts: &[0, 2],
            lines: &picked,
            direction: Direction::LeftToRight,
        },
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::Encode(_))));
    assert!(out.is_empty());
}

#[test]
fn a_malformed_tail_on_a_picked_source_line_is_an_error() {
    let target = utf16le(&[("a", LineEnding::Lf)]);
    let source = [b'a', 0xE2, 0x82, b'\n'];
    let hunk = hashed(
        Hunk {
            left_lines: 0..1,
            left_bytes: 0..4,
            right_lines: 0..1,
            right_bytes: 2..u64::try_from(target.len()).unwrap(),
        },
        &source,
        &target,
    );
    let rows = [Row::Changed { left: 0, right: 0 }];
    let picked = picks([0]);
    let entries = [PickedHunk {
        hunk: &hunk,
        rows: &rows,
    }];
    let mut out = Vec::new();
    let result = apply_picked(
        &laid(&target),
        &view(&source),
        Picked {
            facts: TargetFacts {
                dominant: LineEnding::Lf,
                final_newline: true,
            },
            hunks: &entries,
            digests: &hunk.digests,
            left_starts: &[0, 4],
            right_starts: &[2, 4],
            lines: &picked,
            direction: Direction::LeftToRight,
        },
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::Malformed)));
}

fn assert_rejected(case: &Case<'_>) {
    let mut out = Vec::new();
    let result = apply_to(case, &mut out);
    assert!(matches!(&result, Err(ApplyError::Range)), "{result:?}");
    assert!(out.is_empty(), "wrote bytes before refusing");
}

fn insert_fixture() -> (Side, Side, crate::text::Alignment, Vec<Vec<Row>>) {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let right = lines(&[("one", LineEnding::Lf), ("three", LineEnding::Lf)]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let rows = row_sets(&alignment);
    (left, right, alignment, rows)
}

#[test]
fn a_missing_source_line_is_refused() {
    let (left, right, alignment, mut rows) = insert_fixture();
    let at = rows[0]
        .iter()
        .position(|row| matches!(row, Row::Deleted { left: 1 }))
        .unwrap();
    rows[0].remove(at);
    let picked = picks([1]);
    let case = Case {
        left: &left,
        right: &right,
        alignment: &alignment,
        rows: &rows,
        picks: &picked,
        direction: Direction::LeftToRight,
    };
    assert_rejected(&case);
}

#[test]
fn a_repeated_source_line_is_refused() {
    let (left, right, alignment, mut rows) = insert_fixture();
    let at = rows[0]
        .iter()
        .position(|row| matches!(row, Row::Deleted { left: 1 }))
        .unwrap();
    let repeated = rows[0][at];
    rows[0].insert(at, repeated);
    let picked = picks([1]);
    let case = Case {
        left: &left,
        right: &right,
        alignment: &alignment,
        rows: &rows,
        picks: &picked,
        direction: Direction::LeftToRight,
    };
    assert_rejected(&case);
}

#[test]
fn reversed_row_order_is_refused() {
    let left = lines(&[
        ("aaa", LineEnding::Lf),
        ("bbb", LineEnding::Lf),
        ("ccc", LineEnding::Lf),
    ]);
    let right = lines(&[
        ("aaaX", LineEnding::Lf),
        ("bbbX", LineEnding::Lf),
        ("cccX", LineEnding::Lf),
    ]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let mut rows = row_sets(&alignment);
    assert!(rows[0].len() >= 3);
    rows[0].swap(1, 2);
    let picked = picks([1]);
    let case = Case {
        left: &left,
        right: &right,
        alignment: &alignment,
        rows: &rows,
        picks: &picked,
        direction: Direction::LeftToRight,
    };
    assert_rejected(&case);
}

#[test]
fn a_start_range_outside_the_hunk_is_refused() {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let mut right = lines(&[
        ("one", LineEnding::Lf),
        ("TWO", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    let rows = row_sets(&alignment);
    let end = alignment.hunks[0].right_lines.end;
    right.starts[end] += 1;
    let picked = picks([1]);
    let case = Case {
        left: &left,
        right: &right,
        alignment: &alignment,
        rows: &rows,
        picks: &picked,
        direction: Direction::LeftToRight,
    };
    assert_rejected(&case);
}
