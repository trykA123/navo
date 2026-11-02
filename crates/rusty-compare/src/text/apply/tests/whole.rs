use super::{apply_whole, bom_lines, laid, lines, utf16le, view, whole_hunk, wide_side};
use crate::text::diff::Hunk;
use crate::text::side::LineEnding;
use crate::text::{ApplyError, Direction, TargetFacts, apply_hunk};

#[test]
fn a_whole_hunk_round_trips_utf8() {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let right = lines(&[
        ("one", LineEnding::Lf),
        ("TWO", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let hunk = whole_hunk(1..2, &left.starts, 1..2, &right.starts);
    let out = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(out, left.bytes);
}

#[test]
fn a_whole_hunk_keeps_a_utf16le_bom_and_the_target_encoding() {
    let wide = utf16le(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let source = lines(&[
        ("one", LineEnding::Lf),
        ("TWO", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let hunk = Hunk {
        left_lines: 0..3,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..3,
        right_bytes: 2..u64::try_from(wide.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &laid(&wide),
        &view(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        utf16le(&[
            ("one", LineEnding::Lf),
            ("TWO", LineEnding::Lf),
            ("three", LineEnding::Lf),
        ])
    );
}

#[test]
fn a_whole_hunk_keeps_crlf() {
    let left = lines(&[
        ("one", LineEnding::CrLf),
        ("two", LineEnding::CrLf),
        ("three", LineEnding::CrLf),
    ]);
    let right = lines(&[
        ("one", LineEnding::CrLf),
        ("TWO", LineEnding::CrLf),
        ("three", LineEnding::CrLf),
    ]);
    let hunk = whole_hunk(1..2, &left.starts, 1..2, &right.starts);
    let out = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(out, left.bytes);
}

#[test]
fn a_mixed_target_keeps_unpicked_endings_and_uses_the_dominant_one() {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("TWO", LineEnding::Lf),
        ("three", LineEnding::CrLf),
    ]);
    let right = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::CrLf),
        ("three", LineEnding::CrLf),
    ]);
    let hunk = whole_hunk(1..2, &left.starts, 1..2, &right.starts);
    let out = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(
        out,
        lines(&[
            ("one", LineEnding::Lf),
            ("TWO", LineEnding::CrLf),
            ("three", LineEnding::CrLf),
        ])
        .bytes
    );
}

#[test]
fn an_unterminated_target_last_line_stays_unterminated() {
    let left = lines(&[("one", LineEnding::Lf), ("TWO", LineEnding::Lf)]);
    let right = lines(&[("one", LineEnding::Lf), ("two", LineEnding::None)]);
    let hunk = whole_hunk(1..2, &left.starts, 1..2, &right.starts);
    let out = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(
        out,
        lines(&[("one", LineEnding::Lf), ("TWO", LineEnding::None)]).bytes
    );
}

#[test]
fn an_insertion_at_an_unterminated_end_adds_the_separator() {
    let left = lines(&[("one", LineEnding::Lf), ("two", LineEnding::None)]);
    let right = lines(&[("one", LineEnding::None)]);
    let hunk = whole_hunk(1..2, &left.starts, 1..1, &right.starts);
    let out = apply_whole(&left, &right, &hunk, Direction::LeftToRight);
    assert_eq!(
        out,
        lines(&[("one", LineEnding::Lf), ("two", LineEnding::None)]).bytes
    );
}

#[test]
fn a_whole_hunk_copies_from_right_to_left() {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let right = lines(&[
        ("one", LineEnding::Lf),
        ("TWO", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let hunk = whole_hunk(1..2, &left.starts, 1..2, &right.starts);
    let out = apply_whole(&left, &right, &hunk, Direction::RightToLeft);
    assert_eq!(out, right.bytes);
}

#[test]
fn a_hunk_at_offset_zero_keeps_the_target_bom() {
    let mut target = vec![0xEF, 0xBB, 0xBF];
    target.extend_from_slice(b"a\nb\n");
    let source = lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..2,
        right_bytes: 0..u64::try_from(target.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &laid(&target),
        &view(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    let mut expected = vec![0xEF, 0xBB, 0xBF];
    expected.extend_from_slice(&source.bytes);
    assert_eq!(out, expected);
}

#[test]
fn a_malformed_utf8_source_into_utf16_is_refused() {
    let wide = utf16le(&[("a", LineEnding::Lf)]);
    let source = [b'a', 0xFF, b'\n'];
    let hunk = Hunk {
        left_lines: 0..1,
        left_bytes: 0..3,
        right_lines: 0..1,
        right_bytes: 2..u64::try_from(wide.len()).unwrap(),
    };
    let mut out = Vec::new();
    let result = apply_hunk(
        &laid(&wide),
        &view(&source),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::Malformed)));
}

#[test]
fn a_utf8_bom_source_is_skipped_for_a_whole_hunk() {
    let target = lines(&[("a", LineEnding::Lf), ("b", LineEnding::Lf)]);
    let source = bom_lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..2,
        right_bytes: 0..u64::try_from(target.bytes.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &view(&target.bytes),
        &laid(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]).bytes
    );
}

#[test]
fn a_utf8_bom_source_keeps_the_target_bom() {
    let mut target = vec![0xEF, 0xBB, 0xBF];
    target.extend_from_slice(&lines(&[("a", LineEnding::Lf), ("b", LineEnding::Lf)]).bytes);
    let source = bom_lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..2,
        right_bytes: 0..u64::try_from(target.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &laid(&target),
        &laid(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    let mut expected = vec![0xEF, 0xBB, 0xBF];
    expected.extend_from_slice(&lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]).bytes);
    assert_eq!(out, expected);
}

#[test]
fn a_utf8_bom_source_is_skipped_when_transcoding() {
    let target = utf16le(&[("a", LineEnding::Lf), ("b", LineEnding::Lf)]);
    let source = bom_lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..2,
        right_bytes: 2..u64::try_from(target.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &laid(&target),
        &laid(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        utf16le(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)])
    );
}

#[test]
fn a_utf16le_bom_source_is_skipped_when_transcoding() {
    let source = wide_side(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]);
    let target = lines(&[("a", LineEnding::Lf), ("b", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..2,
        right_bytes: 0..u64::try_from(target.bytes.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &view(&target.bytes),
        &laid(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        lines(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]).bytes
    );
}

#[test]
fn a_utf16le_bom_source_keeps_the_target_bom() {
    let source = wide_side(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)]);
    let target = utf16le(&[("a", LineEnding::Lf), ("b", LineEnding::Lf)]);
    let hunk = Hunk {
        left_lines: 0..2,
        left_bytes: 0..u64::try_from(source.bytes.len()).unwrap(),
        right_lines: 0..2,
        right_bytes: 2..u64::try_from(target.len()).unwrap(),
    };
    let mut out = Vec::new();
    apply_hunk(
        &laid(&target),
        &laid(&source.bytes),
        &hunk,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: true,
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(
        out,
        utf16le(&[("a", LineEnding::Lf), ("B", LineEnding::Lf)])
    );
}
