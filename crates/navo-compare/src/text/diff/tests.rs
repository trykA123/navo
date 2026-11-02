use std::mem::size_of;
use std::time::{Duration, Instant};

use super::*;
use crate::text::normalise::{Whitespace, normalise};
use crate::text::side::LineEnding;

struct View {
    lines: Vec<Line>,
    starts: Vec<u64>,
    bom: bool,
}

impl View {
    fn utf8(lines: Vec<Line>) -> Self {
        let bom = lines.first().is_some_and(|line| line.bom);
        let bom_len = if bom { 3 } else { 0 };
        let mut starts = vec![bom_len as u64];
        let mut at = bom_len;
        for line in &lines {
            at += line.text.len() + line.ending.as_str().len();
            starts.push(at as u64);
        }
        Self { lines, starts, bom }
    }

    fn source(lines: Vec<Line>, starts: Vec<u64>, bom: bool) -> Self {
        assert_eq!(starts.len(), lines.len() + 1);
        Self { lines, starts, bom }
    }

    fn set(&self) -> LineSet<'_> {
        LineSet {
            lines: &self.lines,
            starts: &self.starts,
            bom: self.bom,
        }
    }
}

fn line(text: &str, ending: LineEnding) -> Line {
    Line {
        text: text.to_owned(),
        ending,
        bom: false,
    }
}

fn lines(values: &[(&str, LineEnding)]) -> Vec<Line> {
    values
        .iter()
        .map(|(text, ending)| line(text, *ending))
        .collect()
}

fn equal(left: usize, right: usize) -> Row {
    Row::Equal { left, right }
}

fn deleted(left: usize) -> Row {
    Row::Deleted { left }
}

fn inserted(right: usize) -> Row {
    Row::Inserted { right }
}

#[test]
fn insertion_has_one_hunk_with_exact_ranges() {
    let left = View::utf8(lines(&[("a", LineEnding::Lf), ("c", LineEnding::None)]));
    let right = View::utf8(lines(&[
        ("a", LineEnding::Lf),
        ("b", LineEnding::Lf),
        ("c", LineEnding::None),
    ]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert!(!alignment.raw_equal);
    assert!(!alignment.display_equal);
    assert_eq!(alignment.fallback, None);
    assert_eq!(alignment.rows, vec![equal(0, 0), inserted(1), equal(1, 2)]);
    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 1..1,
            right_lines: 1..2,
            left_bytes: 2..2,
            right_bytes: 2..4,
        }]
    );
}

#[test]
fn deletion_has_a_filler_point_and_exact_ranges() {
    let left = View::utf8(lines(&[
        ("a", LineEnding::Lf),
        ("b", LineEnding::Lf),
        ("c", LineEnding::None),
    ]));
    let right = View::utf8(lines(&[("a", LineEnding::Lf), ("c", LineEnding::None)]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(alignment.rows, vec![equal(0, 0), deleted(1), equal(2, 1)]);
    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 1..2,
            right_lines: 1..1,
            left_bytes: 2..4,
            right_bytes: 2..2,
        }]
    );
}

#[test]
fn a_change_is_a_run_of_deletes_then_inserts() {
    let left = View::utf8(lines(&[
        ("a", LineEnding::Lf),
        ("b", LineEnding::Lf),
        ("c", LineEnding::None),
    ]));
    let right = View::utf8(lines(&[
        ("a", LineEnding::Lf),
        ("B", LineEnding::Lf),
        ("c", LineEnding::None),
    ]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(
        alignment.rows,
        vec![equal(0, 0), deleted(1), inserted(1), equal(2, 2)]
    );
    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 1..2,
            right_lines: 1..2,
            left_bytes: 2..4,
            right_bytes: 2..4,
        }]
    );
}

#[test]
fn diff_fills_changed_rows_from_pairs() {
    let left = View::utf8(lines(&[("let value = 1;", LineEnding::Lf)]));
    let right = View::utf8(lines(&[("let value = 2;", LineEnding::Lf)]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(alignment.rows, vec![Row::Changed { left: 0, right: 0 }]);
}

#[test]
fn rows_keep_unmatched_right_lines_before_a_pair() {
    let left = View::utf8(lines(&[("let value = 1;", LineEnding::Lf)]));
    let right = View::utf8(lines(&[
        ("ZZZZZZZZZZZZZZ", LineEnding::Lf),
        ("let value = 2;", LineEnding::Lf),
    ]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(
        alignment.rows,
        vec![inserted(0), Row::Changed { left: 0, right: 1 }]
    );
}

#[test]
fn a_moved_line_becomes_two_apart_hunks() {
    let left = View::utf8(lines(&[
        ("a", LineEnding::Lf),
        ("b", LineEnding::Lf),
        ("c", LineEnding::Lf),
        ("d", LineEnding::Lf),
    ]));
    let right = View::utf8(lines(&[
        ("b", LineEnding::Lf),
        ("c", LineEnding::Lf),
        ("d", LineEnding::Lf),
        ("a", LineEnding::Lf),
    ]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(alignment.hunks.len(), 2);
    assert!(alignment.hunks[0].left_lines.contains(&0));
    assert!(alignment.hunks[1].right_lines.contains(&3));
    assert!(
        alignment
            .rows
            .iter()
            .any(|row| matches!(row, Row::Deleted { left: 0 }))
    );
    assert!(
        alignment
            .rows
            .iter()
            .any(|row| matches!(row, Row::Inserted { right: 3 }))
    );
}

#[test]
fn display_equal_ignores_whitespace_and_line_endings() {
    let left = View::utf8(vec![line("a  b", LineEnding::CrLf)]);
    let right = View::utf8(vec![line("a b", LineEnding::Lf)]);
    let options = Options {
        whitespace: Whitespace::Changes,
        ignore_line_endings: true,
        ..Options::default()
    };
    let alignment = diff_lines(left.set(), right.set(), &options);

    assert!(!alignment.raw_equal);
    assert!(alignment.display_equal);
    assert!(alignment.hunks.is_empty());
    assert_eq!(alignment.rows, vec![equal(0, 0)]);
    assert_eq!(alignment.fallback, None);
}

#[test]
fn display_equal_ignores_a_bom() {
    let left = View::utf8(vec![Line {
        text: "a".to_owned(),
        ending: LineEnding::None,
        bom: true,
    }]);
    let right = View::utf8(vec![line("a", LineEnding::None)]);
    let options = Options {
        ignore_bom: true,
        ..Options::default()
    };
    let alignment = diff_lines(left.set(), right.set(), &options);

    assert!(!alignment.raw_equal);
    assert!(alignment.display_equal);
    assert!(alignment.hunks.is_empty());
}

#[test]
fn empty_sides_are_equal_and_a_single_empty_side_inserts_everything() {
    let empty = View::source(Vec::new(), vec![0], false);
    let same = diff_lines(empty.set(), empty.set(), &Options::default());
    assert!(same.raw_equal);
    assert!(same.display_equal);
    assert!(same.rows.is_empty());
    assert!(same.hunks.is_empty());

    let right = View::utf8(vec![line("a", LineEnding::Lf)]);
    let alignment = diff_lines(empty.set(), right.set(), &Options::default());
    assert!(!alignment.raw_equal);
    assert!(!alignment.display_equal);
    assert_eq!(alignment.rows, vec![inserted(0)]);
    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 0..0,
            right_lines: 0..1,
            left_bytes: 0..0,
            right_bytes: 0..2,
        }]
    );
}

#[test]
fn bom_only_and_empty_follow_the_bom_option() {
    let bom_only = View::source(Vec::new(), vec![3], true);
    let empty = View::source(Vec::new(), vec![0], false);
    let kept = diff_lines(bom_only.set(), empty.set(), &Options::default());
    assert!(!kept.raw_equal);
    assert!(!kept.display_equal);
    assert!(kept.rows.is_empty());
    assert!(kept.hunks.is_empty());

    let ignored = Options {
        ignore_bom: true,
        ..Options::default()
    };
    let alignment = diff_lines(bom_only.set(), empty.set(), &ignored);
    assert!(!alignment.raw_equal);
    assert!(alignment.display_equal);
    assert!(alignment.rows.is_empty());
    assert!(alignment.hunks.is_empty());
}

#[test]
fn a_final_line_without_a_newline_keeps_its_exact_bytes() {
    let left = View::utf8(lines(&[("a", LineEnding::Lf), ("b", LineEnding::None)]));
    let right = View::utf8(lines(&[("a", LineEnding::Lf), ("c", LineEnding::None)]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 1..2,
            right_lines: 1..2,
            left_bytes: 2..3,
            right_bytes: 2..3,
        }]
    );
}

#[test]
fn byte_ranges_include_crlf_endings() {
    let left = View::utf8(lines(&[("a", LineEnding::CrLf), ("b", LineEnding::CrLf)]));
    let right = View::utf8(lines(&[
        ("a", LineEnding::CrLf),
        ("x", LineEnding::CrLf),
        ("b", LineEnding::CrLf),
    ]));
    let alignment = diff_lines(left.set(), right.set(), &Options::default());

    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 1..1,
            right_lines: 1..2,
            left_bytes: 3..3,
            right_bytes: 3..6,
        }]
    );
}

#[test]
fn source_byte_ranges_follow_the_encoding_and_bom() {
    let left_lines = lines(&[("a", LineEnding::Lf), ("b", LineEnding::Lf)]);
    let right_lines = lines(&[("a", LineEnding::Lf), ("c", LineEnding::Lf)]);
    for (starts, bom, expected) in [
        (vec![2, 6, 10], true, 6..10),
        (vec![0, 2, 4], false, 2..4),
        (vec![3, 5, 7], true, 5..7),
    ] {
        let left = View::source(left_lines.clone(), starts.clone(), bom);
        let right = View::source(right_lines.clone(), starts, bom);
        let alignment = diff_lines(left.set(), right.set(), &Options::default());
        assert_eq!(alignment.hunks.len(), 1, "{expected:?}");
        assert_eq!(alignment.hunks[0].left_bytes, expected);
        assert_eq!(alignment.hunks[0].right_bytes, expected);
    }
}

#[test]
fn the_token_cap_falls_back_to_one_whole_hunk() {
    let left = View::utf8(lines(&[
        ("a", LineEnding::Lf),
        ("b", LineEnding::Lf),
        ("c", LineEnding::Lf),
    ]));
    let right = View::utf8(lines(&[
        ("x", LineEnding::Lf),
        ("y", LineEnding::Lf),
        ("z", LineEnding::Lf),
    ]));
    let alignment = diff_with_limit(left.set(), right.set(), &Options::default(), 4);

    assert_eq!(alignment.fallback, Some(Fallback::TokenCap));
    assert_eq!(
        alignment.rows,
        vec![
            deleted(0),
            deleted(1),
            deleted(2),
            inserted(0),
            inserted(1),
            inserted(2)
        ]
    );
    assert_eq!(
        alignment.hunks,
        vec![Hunk {
            left_lines: 0..3,
            right_lines: 0..3,
            left_bytes: 0..6,
            right_bytes: 0..6,
        }]
    );
}

#[test]
fn streaming_hashes_match_the_normalised_keys() {
    let options = [
        Options::default(),
        Options {
            whitespace: Whitespace::All,
            ..Options::default()
        },
        Options {
            whitespace: Whitespace::Changes,
            ..Options::default()
        },
        Options {
            whitespace: Whitespace::Trailing,
            ..Options::default()
        },
        Options {
            ignore_line_endings: true,
            case: crate::model::CaseRule::Insensitive,
            ..Options::default()
        },
        Options {
            ignore_bom: true,
            ..Options::default()
        },
        Options {
            whitespace: Whitespace::Changes,
            case: crate::model::CaseRule::Insensitive,
            ..Options::default()
        },
    ];
    let cases = [
        ("  a  b  ", LineEnding::CrLf, false),
        ("Straße", LineEnding::Lf, false),
        ("σ", LineEnding::Lf, true),
        ("", LineEnding::None, false),
        ("a\tb", LineEnding::None, true),
    ];
    for options in options {
        for (text, ending, bom) in cases {
            let line = Line {
                text: text.to_owned(),
                ending,
                bom,
            };
            let expected = blake3::hash(normalise(text, ending, bom, &options).as_bytes());
            assert_eq!(key_hash(&line, &options), expected.as_bytes()[..16]);
        }
    }
}

#[test]
fn eight_mib_lines_keep_only_linear_key_state() {
    let size = 8 * 1024 * 1024;
    let left_line = Line {
        text: "a".repeat(size),
        ending: LineEnding::None,
        bom: false,
    };
    let right_line = Line {
        text: "b".repeat(size),
        ending: LineEnding::None,
        bom: false,
    };
    let left = View::source(vec![left_line], vec![0, size as u64], false);
    let right = View::source(vec![right_line], vec![0, size as u64], false);
    let keys = hashes(&left.lines, &Options::default());
    assert_eq!(keys.len(), 1);
    assert!(keys.capacity() * size_of::<[u8; 16]>() <= 64);

    let alignment = diff_lines(left.set(), right.set(), &Options::default());
    assert_eq!(alignment.hunks.len(), 1);
    assert_eq!(alignment.rows.len(), 2);
    assert!(alignment.rows.capacity() <= 4);
    assert!(alignment.hunks.capacity() <= 4);
}

#[test]
#[ignore = "release-only: 10,000-line diff must stay under 50 ms"]
#[allow(
    clippy::print_stderr,
    reason = "the ignored perf test reports its measurement"
)]
fn perf_10k_line_diff_under_50_ms() {
    let mut left = Vec::with_capacity(10_000);
    let mut right = Vec::with_capacity(10_000);
    for index in 0..10_000 {
        let ending = if index == 9_999 {
            LineEnding::None
        } else {
            LineEnding::Lf
        };
        left.push(line(&format!("line {index:05}"), ending));
        let text = if index % 100 == 0 {
            format!("line {index:05} changed")
        } else {
            format!("line {index:05}")
        };
        right.push(line(&text, ending));
    }
    let left = View::utf8(left);
    let right = View::utf8(right);
    let options = Options::default();
    let _ = diff_lines(left.set(), right.set(), &options);
    let mut samples = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        let alignment = diff_lines(left.set(), right.set(), &options);
        samples.push(started.elapsed());
        assert_eq!(alignment.hunks.len(), 100);
    }
    samples.sort_unstable();
    let median = samples[1];
    eprintln!(
        "10,000-line release diff: {:.3} ms",
        median.as_secs_f64() * 1_000.0
    );
    assert!(
        median < Duration::from_millis(50),
        "10,000-line diff took {median:?}"
    );
}
