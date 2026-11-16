//! Tests for the wrap index: identity, round trips, unified order, long lines and speed.

use std::time::{Duration, Instant};

use super::*;
use crate::compare_diff::hooks::{DiffRow, HunkView, PairFacts, ViewedKey};
use crate::compare_diff::long::LONG_LINE;
use crate::compare_diff::rows::{ShownRow, pair_rows, row_count};
use crate::compare_diff::visual_lines;

/// A pair with the given side lines, rows and hunk row ranges.
fn pair(
    left: Vec<String>,
    right: Vec<String>,
    rows: Vec<DiffRow>,
    hunks: &[(u32, u32)],
) -> PairText {
    let hunk = |&(start, end): &(u32, u32)| HunkView {
        rows: start..end,
        left_bytes: 0..0,
        right_bytes: 0..0,
        digest: None,
        added: 0,
        removed: 0,
    };
    PairText {
        left,
        right,
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        rows,
        hunks: hunks.iter().map(hunk).collect(),
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

/// `n` columns of "x".
fn line(n: usize) -> String {
    "x".repeat(n)
}

/// Lines of the given column counts.
fn lines(cols: &[usize]) -> Vec<String> {
    cols.iter().map(|&n| line(n)).collect()
}

/// Equal, changed, deleted, inserted, equal: one hunk over rows 1..4, wrap 10.
fn mixed() -> PairText {
    use DiffRow::{Changed, Deleted, Equal, Inserted};
    let rows = vec![
        Equal(0, 0),
        Changed(1, 1),
        Deleted(2),
        Inserted(2),
        Equal(3, 3),
    ];
    let left = lines(&[5, 25, 31, 4]);
    let right = lines(&[5, 12, 40, 4]);
    pair(left, right, rows, &[(1, 4)])
}

/// The items the shown row takes, read from the text pair_rows draws (the oracle).
fn drawn_lines(row: &ShownRow<'_>, cols: &LineCols, wrap: u32) -> u32 {
    let n = match (row.right, row.left) {
        (Some((i, _)), _) => cols.right(i),
        (None, Some((i, _))) => cols.left(i),
        (None, None) => 0,
    };
    visual_lines(n, wrap)
}

#[test]
fn flat_is_the_identity() {
    let index = WrapIndex::flat(7);
    assert_eq!(index.total(), 7);
    for row in 0..7 {
        assert_eq!(index.first_of(row), row);
        assert_eq!(index.at(row), (row, 0));
        assert_eq!(index.lines_of(row), 1);
    }
    assert_eq!(index.first_of(7), 7);
    assert_eq!(index.first_of(99), 7);
    assert_eq!(index.at(99), (6, 0));
    assert_eq!(index.lines_of(7), 0);
}

#[test]
fn an_empty_pair_has_no_items() {
    let index = WrapIndex::flat(0);
    assert_eq!(index.total(), 0);
    assert_eq!(index.first_of(0), 0);
    assert_eq!(index.at(0), (0, 0));
}

#[test]
fn side_by_side_takes_the_taller_side_and_round_trips() {
    let text = mixed();
    let cols = LineCols::measure(&text);
    let index = WrapIndex::build(&text, Layout::SideBySide, &cols, 10);
    let heights: Vec<u32> = (0..5).map(|r| index.lines_of(r)).collect();
    assert_eq!(heights, [1, 3, 4, 4, 1]);
    assert_eq!(index.total(), 13);
    let mut next = 0;
    for row in 0..5 {
        assert_eq!(index.first_of(row), next);
        for sub in 0..index.lines_of(row) {
            assert_eq!(index.at(next), (row, sub));
            next += 1;
        }
        assert_eq!(index.at(index.first_of(row)), (row, 0));
    }
    assert_eq!(index.first_of(5), 13);
    assert_eq!(index.at(500), (4, 0));
}

#[test]
fn unified_lists_deleted_lines_before_inserted_ones() {
    use DiffRow::Changed;
    let rows = vec![Changed(0, 0), Changed(1, 1)];
    let text = pair(lines(&[20, 20]), lines(&[3, 3]), rows, &[(0, 2)]);
    let cols = LineCols::measure(&text);
    let unified = WrapIndex::build(&text, Layout::Unified, &cols, 10);
    let heights: Vec<u32> = (0..4).map(|r| unified.lines_of(r)).collect();
    assert_eq!(heights, [2, 2, 1, 1]);
    assert_eq!(unified.total(), 6);
    let side = WrapIndex::build(&text, Layout::SideBySide, &cols, 10);
    assert_eq!(side.total(), 4);
    assert_eq!(unified.at(4), (2, 0));
}

#[test]
fn unified_matches_the_rows_pair_rows_draws() {
    use DiffRow::{Changed, Deleted, Equal, Inserted};
    let rows = vec![
        Equal(0, 0),
        Changed(1, 1),
        Deleted(2),
        Changed(3, 2),
        Equal(4, 3),
        Inserted(4),
        Inserted(5),
        Equal(5, 6),
        Changed(6, 7),
    ];
    let left = lines(&[2, 30, 9, 44, 12, 1, 25]);
    let right = lines(&[2, 8, 33, 15, 40, 41, 3, 22]);
    for hunks in [
        &[(1, 7)][..],
        &[(1, 4), (5, 7)],
        &[(1, 3), (3, 4), (5, 7)],
        &[],
    ] {
        let text = pair(left.clone(), right.clone(), rows.clone(), hunks);
        let cols = LineCols::measure(&text);
        let shown = pair_rows(&text, Layout::Unified, 0..row_count(&text, Layout::Unified));
        let index = WrapIndex::build(&text, Layout::Unified, &cols, 10);
        let want: Vec<u32> = shown.iter().map(|r| drawn_lines(r, &cols, 10)).collect();
        let got: Vec<u32> = (0..shown.len()).map(|r| index.lines_of(r)).collect();
        assert_eq!(got, want, "hunks {hunks:?}");
        assert_eq!(index.lines_of(shown.len()), 0);
        assert_eq!(index.total(), want.iter().sum::<u32>() as usize);
    }
}

#[test]
fn a_long_line_counts_one() {
    let big = "y".repeat(LONG_LINE + 1);
    let rows = vec![DiffRow::Changed(0, 0)];
    let text = pair(vec![big], lines(&[15]), rows, &[(0, 1)]);
    let cols = LineCols::measure(&text);
    let index = WrapIndex::build(&text, Layout::SideBySide, &cols, 10);
    assert_eq!(index.lines_of(0), 2);
    let text = pair(
        vec!["y".repeat(LONG_LINE + 1)],
        lines(&[3]),
        vec![DiffRow::Deleted(0)],
        &[],
    );
    let cols = LineCols::measure(&text);
    let index = WrapIndex::build(&text, Layout::SideBySide, &cols, 10);
    assert_eq!((index.lines_of(0), index.total()), (1, 1));
}

#[test]
fn a_wrap_of_zero_is_treated_as_one_column() {
    let text = pair(
        lines(&[3]),
        lines(&[1]),
        vec![DiffRow::Changed(0, 0)],
        &[(0, 1)],
    );
    let cols = LineCols::measure(&text);
    assert_eq!(
        WrapIndex::build(&text, Layout::SideBySide, &cols, 0).total(),
        3
    );
}

const ROWS: usize = 200_000;

/// 200,000 equal rows; every 200th has a 95-column line on both sides (10 items at wrap 10).
fn big() -> PairText {
    let rows = (0..ROWS as u32).map(|i| DiffRow::Equal(i, i)).collect();
    let tall = |i: usize| if i % 200 == 7 { 95 } else { 4 };
    let left = (0..ROWS).map(|i| line(tall(i))).collect();
    let right = (0..ROWS).map(|i| line(tall(i))).collect();
    pair(left, right, rows, &[])
}

#[test]
fn two_hundred_thousand_rows_with_a_thousand_tall_ones() {
    let text = big();
    let cols = LineCols::measure(&text);
    for layout in [Layout::SideBySide, Layout::Unified] {
        let index = WrapIndex::build(&text, layout, &cols, 10);
        assert_eq!(index.total(), ROWS + 1_000 * 9);
        assert_eq!(index.first_of(0), 0);
        assert_eq!(index.first_of(7), 7);
        assert_eq!(index.first_of(8), 7 + 10);
        assert_eq!(index.first_of(ROWS), index.total());
        assert_eq!(index.at(index.total() - 1), (ROWS - 1, 0));
        assert_eq!(index.at(7 + 9), (7, 9));
        assert_eq!(index.at(7 + 10), (8, 0));
        assert_eq!(index.lines_of(207), 10);
        assert_eq!(index.at(index.first_of(199_807) + 3), (199_807, 3));
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "timing: release only; run with --release --run-ignored all"
)]
#[allow(clippy::print_stdout, reason = "the timing is reported on the CI log")]
fn building_two_hundred_thousand_rows_takes_under_five_ms() {
    let text = big();
    let cols = LineCols::measure(&text);
    let budget = Duration::from_millis(5);
    for layout in [Layout::SideBySide, Layout::Unified] {
        let took = (0..9)
            .map(|_| {
                let started = Instant::now();
                let index = WrapIndex::build(&text, layout, &cols, 10);
                let took = started.elapsed();
                assert_eq!(index.total(), ROWS + 9_000);
                took
            })
            .min()
            .unwrap_or_default();
        println!("wrap index build {layout:?}: {took:?}");
        assert!(took < budget, "{layout:?}: {took:?} breaks the 5 ms budget");
    }
}
