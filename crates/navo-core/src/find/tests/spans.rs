use std::ops::Range;

use crate::find::{Compiled, FindQuery};

/// Compiles `query`, collects the spans of `text`, and checks every range contract.
fn spans_of(query: &str, text: &str) -> Vec<Range<usize>> {
    let compiled = Compiled::new(&FindQuery::parse(query)).unwrap();
    let mut out = Vec::new();
    compiled.spans(text, &mut out);
    for range in &out {
        assert!(range.start <= range.end, "{range:?} is reversed");
        assert!(range.end <= text.len(), "{range:?} runs past the text");
        assert!(
            text.is_char_boundary(range.start),
            "{range:?} starts mid-character"
        );
        assert!(
            text.is_char_boundary(range.end),
            "{range:?} ends mid-character"
        );
    }
    out
}

/// The slices the spans highlight, which panics when a range is not a char boundary.
fn matched<'a>(query: &str, text: &'a str) -> Vec<&'a str> {
    spans_of(query, text)
        .into_iter()
        .map(|range| &text[range])
        .collect()
}

#[test]
fn literal_spans_cover_each_ascii_occurrence() {
    let text = "hello world, hello again";
    assert_eq!(spans_of("case:on hello", text), [0..5, 13..18]);
    assert_eq!(matched("case:on hello", text), ["hello", "hello"]);
    assert!(spans_of("case:on absent", text).is_empty());
}

#[test]
fn word_on_skips_substring_matches() {
    let text = "cat concat cat scatter";
    assert_eq!(spans_of("case:on word:on cat", text), [0..3, 11..14]);
    assert_eq!(matched("case:on word:on cat", text), ["cat", "cat"]);
}

#[test]
fn folded_spans_land_on_character_boundaries() {
    let text = "Die Größe zählt, die größe auch, GRÖSSE nicht";
    let ranges = spans_of("größe", text);
    assert_eq!(ranges.len(), 2);
    assert_eq!(&text[ranges[0].clone()], "Größe");
    assert_eq!(&text[ranges[1].clone()], "größe");
    assert_eq!(matched("GRÖSSE", text), ["GRÖSSE"]);

    // A char whose lower-case form grows to two chars still maps back to its own bytes.
    let dotted = "İstanbul am Bosporus";
    assert_eq!(matched("STANBUL", dotted), ["stanbul"]);
    assert_eq!(matched("bosporus", dotted), ["Bosporus"]);
}

#[test]
fn regex_spans_cover_each_match() {
    let text = "id 42 and 7 end";
    assert_eq!(matched(r"re:\d+", text), ["42", "7"]);
    assert!(spans_of(r"re:\d{3}", text).is_empty());
}

#[test]
fn hex_spans_cover_the_matching_byte_pairs() {
    let row = "00000000  4D 5A 90 00";
    assert_eq!(spans_of("hex:4D5A", row), vec![10..15]);
    assert_eq!(matched("hex:4D5A", row), ["4D 5A"]);
    assert_eq!(matched("hex:4D5A90", row), ["4D 5A 90"]);
    assert!(spans_of("hex:4D5B", row).is_empty());

    let doubled = "00000010  4D 5A 4D 5A 90";
    assert_eq!(spans_of("hex:4D5A", doubled), [10..15, 16..21]);
    assert_eq!(matched("hex:4D5A", doubled), ["4D 5A", "4D 5A"]);

    let offset_row = "00000010  00 01 02 03 04 4D 5A 07 08 09 0A 0B 0C 0D 0E 0F";
    assert_eq!(matched("hex:4D5A", offset_row), ["4D 5A"]);
}

#[test]
fn hex_spans_highlight_the_row_prefix_of_a_long_needle() {
    let row = "00000000  4D 5A 90 00 03 00 00 00 04 00 00 00 FF FF 00 00";
    assert_eq!(
        matched("hex:4D5A90000300000004000000FFFF0000AABBCCDD", row),
        ["4D 5A 90 00 03 00 00 00 04 00 00 00 FF FF 00 00"]
    );
}

#[test]
fn hex_spans_highlight_a_short_needle_wrapping_the_row_end() {
    let row = "00000010  00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 4D 5A";
    assert_eq!(matched("hex:4D5A9000", row), ["4D 5A"]);
    assert!(spans_of("hex:4D5B9000", row).is_empty());
}

#[test]
fn hex_spans_cover_rows_past_four_gibibytes() {
    let row = "100000000  4D 5A 90 00";
    assert_eq!(spans_of("hex:4D5A", row), vec![11..16]);
    assert_eq!(matched("hex:4D5A", row), ["4D 5A"]);
}

#[test]
fn empty_query_clears_existing_ranges() {
    let compiled = Compiled::new(&FindQuery::parse("")).unwrap();
    let mut out = vec![0..3, 5..8];
    compiled.spans("some text", &mut out);
    assert!(out.is_empty());
}
