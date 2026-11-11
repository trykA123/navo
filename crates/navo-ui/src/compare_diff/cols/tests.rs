//! Tests for the column maths: tab stops, wide characters, the widest line and the cut.

use super::*;
use crate::compare_diff::hooks::{DiffRow, HunkView, PairFacts, ViewedKey};
use crate::compare_diff::long::LONG_LINE;

/// A pair with the given side lines and no rows.
fn pair(left: &[&str], right: &[&str]) -> PairText {
    PairText {
        left: left.iter().map(|s| (*s).to_owned()).collect(),
        right: right.iter().map(|s| (*s).to_owned()).collect(),
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        rows: Vec::<DiffRow>::new(),
        hunks: Vec::<HunkView>::new(),
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

/// The cut text of `line` for `cols`, without runs.
fn text_of(line: &str, cols: Range<u32>) -> String {
    cut::<()>(line, &[], cols).text
}

#[test]
fn tab_runs_to_the_next_stop_of_four() {
    assert_eq!(char_cols('\t', 0), 4);
    assert_eq!(char_cols('\t', 1), 3);
    assert_eq!(char_cols('\t', 3), 1);
    assert_eq!(char_cols('\t', 4), 4);
    assert_eq!(line_cols("a\tb"), 5);
}

#[test]
fn wide_characters_take_two_and_controls_one() {
    assert_eq!(line_cols("日本"), 4);
    assert_eq!(char_cols('\u{7}', 0), 1);
    assert_eq!(char_cols('\u{301}', 0), 0);
    assert_eq!(line_cols("e\u{301}"), 1);
}

#[test]
fn ascii_fast_path_equals_the_slow_path() {
    let line = "fn main() { println!(\"hello, world\"); }";
    assert_eq!(line_cols(line), slow_cols(line));
    assert_eq!(line_cols(line), line.len() as u32);
    assert_eq!(line_cols(""), 0);
}

#[test]
fn visual_lines_round_up_and_never_reach_zero() {
    assert_eq!(visual_lines(0, 10), 1);
    assert_eq!(visual_lines(10, 10), 1);
    assert_eq!(visual_lines(11, 10), 2);
    assert_eq!(visual_lines(5, 0), 5);
}

#[test]
fn measure_finds_the_widest_and_reads_out_of_range_as_zero() {
    let cols = LineCols::measure(&pair(&["ab", "日本語"], &["a\tb"]));
    assert_eq!((cols.left(0), cols.left(1), cols.right(0)), (2, 6, 5));
    assert_eq!((cols.left(2), cols.right(1)), (0, 0));
    assert_eq!(cols.widest(), 6);
}

#[test]
fn measure_ignores_lines_over_the_long_limit() {
    let long = "x".repeat(LONG_LINE + 1);
    let cols = LineCols::measure(&pair(&[&long, "abc"], &[]));
    assert_eq!(cols.left(0), 0);
    assert_eq!(cols.widest(), 3);
}

#[test]
fn cut_keeps_the_characters_that_start_in_range() {
    assert_eq!(text_of("abcdef", 2..4), "cd");
    assert_eq!(text_of("abc", 0..10), "abc");
    assert_eq!(text_of("abc", 5..9), "");
    assert_eq!(text_of("abc", 1..1), "");
}

#[test]
fn cut_turns_tabs_and_controls_into_spaces() {
    assert_eq!(text_of("a\tb", 0..10), "a   b");
    assert_eq!(text_of("a\u{7}b", 0..10), "a b");
}

#[test]
fn cut_through_a_tab_pads_the_front() {
    // The tab covers columns 1..4; a cut at column 2 drops it and pads two spaces.
    assert_eq!(text_of("a\tbc", 2..9), "  bc");
    // A tab that starts inside the range stays whole, even past the end.
    assert_eq!(text_of("a\tbc", 0..2), "a   ");
}

#[test]
fn cut_through_a_wide_character_pads_one_space() {
    // "日" covers columns 0..2; a cut at column 1 drops it and pads one space.
    assert_eq!(text_of("日本", 1..4), " 本");
    assert_eq!(text_of("日本", 0..3), "日本");
    assert_eq!(text_of("日本", 0..1), "日");
}

#[test]
fn cut_keeps_a_zero_width_mark_with_its_base() {
    assert_eq!(text_of("e\u{301}x", 0..1), "e\u{301}");
    assert_eq!(text_of("xe\u{301}y", 2..3), "y");
    assert_eq!(text_of("e\u{301}x", 1..3), "x");
}

#[test]
fn runs_follow_their_characters() {
    let runs = [(1..3, 'a'), (4..5, 'b')];
    let shown = cut("0123456", &runs, 2..6);
    assert_eq!(shown.text, "2345");
    assert_eq!(shown.runs, vec![(0..1, 'a'), (2..3, 'b')]);
}

#[test]
fn runs_over_a_cut_tab_cover_the_kept_part_only() {
    let runs = [(1..3, 'a')];
    // "a\tbc": the tab is dropped by a cut at 2, so only "b" (byte 2) is shown, after 2 pads.
    let shown = cut("a\tbc", &runs, 2..9);
    assert_eq!(shown.text, "  bc");
    assert_eq!(shown.runs, vec![(2..3, 'a')]);
    // A run over a kept tab covers all its spaces.
    let shown = cut("a\tb", &[(1..2, 'a')], 0..9);
    assert_eq!(shown.runs, vec![(1..4, 'a')]);
}

#[test]
fn runs_past_the_end_and_outside_the_cut_are_dropped() {
    let runs = [(0..2, 'a'), (6..9, 'b')];
    let shown = cut("abcdef", &runs, 2..5);
    assert_eq!(shown.text, "cde");
    assert!(shown.runs.is_empty());
    let shown = cut("abc", &[(1..40, 'x')], 0..9);
    assert_eq!(shown.runs, vec![(1..3, 'x')]);
}

#[test]
fn runs_move_with_wide_characters() {
    let shown = cut("a日本", &[(1..4, 'w')], 0..9);
    assert_eq!(shown.text, "a日本");
    assert_eq!(shown.runs, vec![(1..4, 'w')]);
}
