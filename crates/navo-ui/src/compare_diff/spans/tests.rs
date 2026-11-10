//! Tests of the span cache: only unknown rows are asked, spans are found by line, old rows go.

use super::{MAX_ROWS, SpanCache};
use crate::compare_diff::{DiffRow, HunkView, PairFacts, PairText, RowSpans, ViewedKey};

/// A list of one byte range.
fn one(start: u32, end: u32) -> Vec<std::ops::Range<u32>> {
    std::iter::once(start..end).collect()
}

/// A pair of `len` changed rows (line n faces line n) with no text.
fn pair(len: u32) -> PairText {
    PairText {
        left: Vec::new(),
        right: Vec::new(),
        rows: (0..len).map(|at| DiffRow::Changed(at, at)).collect(),
        hunks: vec![HunkView {
            rows: 0..len,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 0,
            removed: 0,
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

#[test]
fn spans_ask_only_for_rows_not_asked_before() {
    let mut cache = SpanCache::default();
    assert_eq!(cache.request(10..20), Some(10..20));
    assert_eq!(cache.request(10..20), None, "all known");
    assert_eq!(cache.request(15..25), Some(20..25), "only the new rows");
    assert_eq!(cache.request(5..12), Some(5..10));
}

#[test]
fn spans_are_found_by_line_and_other_rows_are_empty() {
    let mut cache = SpanCache::default();
    cache.request(0..4);
    let found = RowSpans {
        row: 2,
        left: one(1, 3),
        right: vec![0..2, 5..6],
    };
    cache.insert(&pair(4), vec![found]);
    assert_eq!(cache.left(2), one(1, 3).as_slice());
    assert_eq!(cache.right(2), &[0..2, 5..6]);
    assert!(cache.left(1).is_empty());
}

#[test]
fn spans_keep_at_most_4096_rows_and_drop_the_oldest() {
    let mut cache = SpanCache::default();
    let limit = MAX_ROWS as u32;
    cache.request(0..limit);
    let first = RowSpans {
        row: 0,
        left: one(0, 1),
        right: one(0, 1),
    };
    cache.insert(&pair(limit), vec![first]);
    assert_eq!(cache.known(), MAX_ROWS);
    assert_eq!(cache.request(limit..limit + 10), Some(limit..limit + 10));
    assert_eq!(cache.known(), MAX_ROWS);
    assert!(cache.left(0).is_empty(), "row 0 was evicted");
    assert_eq!(
        cache.request(0..1),
        Some(0..1),
        "an evicted row is asked again"
    );
}
