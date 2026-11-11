//! Tests for the row cut: cut text and runs, long lines left whole, and continued rows.

use super::{LineAt, RowCut, lead_row, shown_line};
use crate::compare_diff::body::{BodyLook, Side};
use crate::compare_diff::long::LONG_LINE;
use crate::compare_diff::{
    DiffRow, Layout, PairFacts, PairText, RowSpans, ShownKind, ShownRow, SpanCache, ViewedKey,
};
use crate::tokens::{Mode, Theme};

/// A pair whose left side is `lines`, with nothing else.
fn pair(lines: Vec<String>) -> PairText {
    PairText {
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        left: lines,
        right: vec![],
        rows: vec![DiffRow::Changed(0, 0)],
        hunks: vec![],
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

/// A span cache with left line 0 changed at `run`.
fn spans(text: &PairText, run: std::ops::Range<u32>) -> SpanCache {
    let mut cache = SpanCache::default();
    cache.request(0..1);
    let found = RowSpans {
        row: 0,
        left: vec![run],
        right: vec![],
    };
    cache.insert(text, vec![found]);
    cache
}

/// The byte ranges of `shown`'s runs.
fn ranges(
    shown: &crate::compare_diff::cols::Shown<gpui_kit::HighlightStyle>,
) -> Vec<std::ops::Range<usize>> {
    shown.runs.iter().map(|(run, _)| run.clone()).collect()
}

#[test]
fn shown_line_cuts_text_and_runs_to_the_columns() {
    let t = Theme::new(Mode::Light);
    let text = pair(vec!["abcdefgh".into()]);
    let cache = spans(&text, 2..5);
    let look = BodyLook {
        theme: &t,
        layout: Layout::SideBySide,
        digits: 1,
        current: None,
        spans: Some(&cache),
        ticks: None,
        bytes_here: None,
        syntax: None,
    };
    let at = |line| LineAt {
        kind: ShownKind::Changed,
        side: Side::Left,
        line,
        slot: Side::Left,
    };
    let line = at(Some((0, "abcdefgh")));
    let tint = t.diff.change.fill;
    let whole = shown_line(&look, &line, &RowCut::default()).expect("a line");
    assert_eq!(whole.text, "abcdefgh");
    assert_eq!(ranges(&whole), vec![2..5]);
    let cut = RowCut {
        cols: Some(3..6),
        ..RowCut::default()
    };
    let shown = shown_line(&look, &line, &cut).expect("a line");
    assert_eq!(shown.text, "def");
    assert_eq!(
        ranges(&shown),
        vec![0..2],
        "the run moves with its characters"
    );
    assert_eq!(shown.runs[0].1.background_color, Some(tint));
    assert!(shown_line(&look, &at(None), &cut).is_none());
}

#[test]
fn shown_line_never_cuts_a_line_over_a_mebibyte() {
    let t = Theme::new(Mode::Light);
    let long = "a".repeat(LONG_LINE + 1);
    let look = BodyLook {
        theme: &t,
        layout: Layout::Unified,
        digits: 1,
        current: None,
        spans: None,
        ticks: None,
        bytes_here: None,
        syntax: None,
    };
    let cut = RowCut {
        cols: Some(0..4),
        ..RowCut::default()
    };
    let line = LineAt {
        kind: ShownKind::Changed,
        side: Side::Left,
        line: Some((0, &long)),
        slot: Side::Left,
    };
    let shown = shown_line(&look, &line, &cut).expect("a line");
    assert_eq!(shown.text.len(), long.len());
    let later = RowCut {
        cols: Some(4..8),
        continued: true,
        ..RowCut::default()
    };
    let shown = shown_line(&look, &line, &later).expect("a line");
    assert_eq!(
        shown.text, "",
        "a later wrapped line leaves the long line out"
    );
}

#[test]
fn a_continued_row_has_no_number_tick_or_button() {
    let row = ShownRow {
        kind: ShownKind::Changed,
        left: Some((4, "left")),
        right: Some((7, "right")),
        hunk: Some(0),
        row: 9,
    };
    assert_eq!(lead_row(&row, &RowCut::default()), row);
    let continued = RowCut {
        continued: true,
        ..RowCut::default()
    };
    let lead = lead_row(&row, &continued);
    assert_eq!((lead.left, lead.right), (None, None));
    assert_eq!(
        (lead.kind, lead.hunk, lead.row),
        (row.kind, row.hunk, row.row)
    );
}
