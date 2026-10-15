use proptest::prelude::*;

use super::tests_support::Short;
use super::{LineIndex, MARK_EVERY};

fn indexed(bytes: &[u8]) -> LineIndex {
    let mut index = LineIndex::new();
    index.feed(bytes);
    index.finish();
    index
}

fn lines(count: u64) -> Vec<u8> {
    b"ab\n".repeat(usize::try_from(count).unwrap())
}

fn varied(count: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut seed = 7u32;
    for line in 0..count {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let len = 1 + (seed >> 16) % 160;
        bytes.extend((0..len).map(|i| b'a' + u8::try_from(i % 26).unwrap()));
        if line + 1 < count {
            bytes.push(b'\n');
        }
    }
    bytes
}

fn naive_line_number(bytes: &[u8], offset: usize) -> u64 {
    bytes[..offset].iter().filter(|&&b| b == b'\n').count() as u64 + 1
}

fn naive_line_starts(bytes: &[u8]) -> Vec<u64> {
    let after = bytes
        .iter()
        .enumerate()
        .filter(|&(_, &b)| b == b'\n')
        .map(|(i, _)| i as u64 + 1);
    std::iter::once(0).chain(after).collect()
}

#[test]
fn new_index_is_empty() {
    let index = LineIndex::new();
    assert_eq!(index.scanned(), 0);
    assert_eq!(index.newlines(), 0);
    assert_eq!(index.marks(), 1);
    assert!(!index.is_done());
}

#[test]
fn feed_counts_bytes_and_newlines() {
    let mut index = LineIndex::new();
    index.feed(b"a\nb\n");
    index.feed(b"cc");
    assert_eq!(index.scanned(), 6);
    assert_eq!(index.newlines(), 2);
}

#[test]
fn finish_marks_the_index_done() {
    let mut index = LineIndex::new();
    index.feed(b"a\n");
    index.finish();
    assert!(index.is_done());
}

#[test]
fn no_second_mark_before_the_1024th_newline() {
    assert_eq!(indexed(&lines(MARK_EVERY - 1)).marks(), 1);
}

#[test]
fn the_1024th_newline_records_a_mark() {
    assert_eq!(indexed(&lines(MARK_EVERY)).marks(), 2);
}

#[test]
fn marks_grow_every_1024_lines() {
    assert_eq!(indexed(&lines(MARK_EVERY * 3 + 5)).marks(), 4);
}

#[test]
fn line_number_counts_from_one() {
    let bytes = b"a\nbb\nccc";
    let index = indexed(bytes);
    let src = bytes.as_slice();
    assert_eq!(index.line_number(src, 0).unwrap(), Some(1));
    assert_eq!(index.line_number(src, 2).unwrap(), Some(2));
    assert_eq!(index.line_number(src, 5).unwrap(), Some(3));
    assert_eq!(index.line_number(src, 7).unwrap(), Some(3));
}

#[test]
fn line_number_newline_belongs_to_its_line() {
    let bytes = b"a\nbb\nccc";
    let index = indexed(bytes);
    assert_eq!(index.line_number(bytes.as_slice(), 1).unwrap(), Some(1));
    assert_eq!(index.line_number(bytes.as_slice(), 4).unwrap(), Some(2));
}

#[test]
fn line_number_past_the_end_when_done_is_the_last_line() {
    let bytes = b"a\nbb\nccc";
    let index = indexed(bytes);
    assert_eq!(index.line_number(bytes.as_slice(), 8).unwrap(), Some(3));
    assert_eq!(index.line_number(bytes.as_slice(), 500).unwrap(), Some(3));
}

#[test]
fn line_number_is_unknown_past_the_scanned_part() {
    let bytes = b"a\nbb\nccc";
    let mut index = LineIndex::new();
    index.feed(&bytes[..4]);
    let src = bytes.as_slice();
    assert_eq!(index.line_number(src, 3).unwrap(), Some(2));
    assert_eq!(index.line_number(src, 4).unwrap(), None);
    assert_eq!(index.line_number(src, 7).unwrap(), None);
}

#[test]
fn line_start_finds_each_line() {
    let bytes = b"a\nbb\nccc";
    let index = indexed(bytes);
    let src = bytes.as_slice();
    assert_eq!(index.line_start(src, 1).unwrap(), Some(0));
    assert_eq!(index.line_start(src, 2).unwrap(), Some(2));
    assert_eq!(index.line_start(src, 3).unwrap(), Some(5));
}

#[test]
fn line_start_treats_line_zero_as_line_one() {
    let bytes = b"a\nbb\nccc";
    let index = indexed(bytes);
    assert_eq!(index.line_start(bytes.as_slice(), 0).unwrap(), Some(0));
}

#[test]
fn line_start_beyond_the_last_line_when_done_is_none() {
    let bytes = b"a\nbb\nccc";
    let index = indexed(bytes);
    assert_eq!(index.line_start(bytes.as_slice(), 4).unwrap(), None);
    assert_eq!(index.line_start(bytes.as_slice(), 1000).unwrap(), None);
}

#[test]
fn line_start_is_unknown_before_it_is_scanned() {
    let bytes = b"a\nbb\nccc";
    let mut index = LineIndex::new();
    index.feed(&bytes[..3]);
    let src = bytes.as_slice();
    assert_eq!(index.line_start(src, 2).unwrap(), Some(2));
    assert_eq!(index.line_start(src, 3).unwrap(), None);
}

#[test]
fn line_number_matches_a_naive_count_in_a_big_file() {
    let bytes = varied(5000);
    let mut index = LineIndex::new();
    for chunk in bytes.chunks(4096) {
        index.feed(chunk);
    }
    index.finish();
    let src = bytes.as_slice();
    for offset in (0..bytes.len()).step_by(97).chain([bytes.len() - 1]) {
        let want = naive_line_number(&bytes, offset);
        let got = index.line_number(src, offset as u64).unwrap();
        assert_eq!(got, Some(want), "offset {offset}");
    }
}

#[test]
fn line_start_matches_a_naive_count_in_a_big_file() {
    let bytes = varied(5000);
    let index = indexed(&bytes);
    let src = bytes.as_slice();
    for (line, &want) in (1u64..).zip(&naive_line_starts(&bytes)) {
        assert_eq!(
            index.line_start(src, line).unwrap(),
            Some(want),
            "line {line}"
        );
    }
    assert_eq!(index.line_start(src, 5001).unwrap(), None);
}

#[test]
fn lookups_around_marks_match_a_naive_count() {
    let bytes = varied(5000);
    let index = indexed(&bytes);
    let src = bytes.as_slice();
    let starts = naive_line_starts(&bytes);
    for line in [1023u64, 1024, 1025, 1026, 2048, 2049, 4096, 4097, 5000] {
        let want = starts[usize::try_from(line - 1).unwrap()];
        assert_eq!(
            index.line_start(src, line).unwrap(),
            Some(want),
            "line {line}"
        );
        assert_eq!(index.line_number(src, want).unwrap(), Some(line));
    }
}

#[test]
fn lookups_loop_over_short_reads() {
    let bytes = varied(3000);
    let index = indexed(&bytes);
    let src = Short::new(&bytes, 7);
    let starts = naive_line_starts(&bytes);
    assert_eq!(index.line_start(&src, 2500).unwrap(), Some(starts[2499]));
    assert_eq!(
        index.line_number(&src, starts[2499] + 1).unwrap(),
        Some(2500)
    );
}

fn newline_heavy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop_oneof![Just(b'\n'), Just(b'a')], 0..5000)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn feeding_in_any_chunks_gives_the_same_index(
        bytes in newline_heavy(),
        cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..12),
    ) {
        let mut points: Vec<usize> = cuts.iter().map(|cut| cut.index(bytes.len() + 1)).collect();
        points.push(0);
        points.push(bytes.len());
        points.sort_unstable();
        let mut chunked = LineIndex::new();
        for pair in points.windows(2) {
            chunked.feed(&bytes[pair[0]..pair[1]]);
        }
        let mut whole = LineIndex::new();
        whole.feed(&bytes);
        prop_assert_eq!(&chunked, &whole);
        chunked.finish();
        whole.finish();
        prop_assert_eq!(chunked, whole);
    }
}
