use std::sync::atomic::AtomicBool;

use super::tests_support::Short;
use super::{ByteSource, Found, Needle, SEARCH_CHUNK, Search, find, parse_hex};

fn text(text: &str, match_case: bool) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case,
    }
}

fn run<S: ByteSource + ?Sized>(src: &S, needle: Needle, from: u64, backward: bool) -> Found {
    let search = Search {
        needle,
        from,
        backward,
    };
    find(src, &search, &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn forward(src: &[u8], needle: &str, from: u64) -> Found {
    run(src, text(needle, true), from, false)
}

fn backward(src: &[u8], needle: &str, from: u64) -> Found {
    run(src, text(needle, true), from, true)
}

const FIRST: usize = SEARCH_CHUNK - 3;
const SECOND: usize = 2 * SEARCH_CHUNK - 2;

fn big() -> Vec<u8> {
    let mut bytes = vec![b'.'; 2 * SEARCH_CHUNK + 100];
    bytes[FIRST..FIRST + 6].copy_from_slice(b"needle");
    bytes[SECOND..SECOND + 6].copy_from_slice(b"needle");
    bytes
}

#[test]
fn parse_hex_reads_spaced_pairs() {
    assert_eq!(parse_hex("4D 5A").unwrap(), vec![0x4D, 0x5A]);
}

#[test]
fn parse_hex_reads_lowercase_without_spaces() {
    assert_eq!(parse_hex("4d5a").unwrap(), vec![0x4D, 0x5A]);
}

#[test]
fn parse_hex_ignores_outer_and_mixed_spaces() {
    assert_eq!(parse_hex(" 4D5A 90 ").unwrap(), vec![0x4D, 0x5A, 0x90]);
}

#[test]
fn parse_hex_rejects_empty_input() {
    assert!(parse_hex("").is_err());
    assert!(parse_hex("   ").is_err());
}

#[test]
fn parse_hex_rejects_an_odd_digit_count() {
    assert!(parse_hex("4D5").is_err());
    assert!(parse_hex("4 D 5").is_err());
}

#[test]
fn parse_hex_rejects_non_hex_chars() {
    assert!(parse_hex("4G").is_err());
    assert!(parse_hex("0x4D").is_err());
    assert!(parse_hex("4D,5A").is_err());
}

#[test]
fn forward_finds_the_first_match_at_or_after_from() {
    let src = b"abcabc".as_slice();
    assert_eq!(forward(src, "abc", 0), Found::At(0));
    assert_eq!(forward(src, "abc", 1), Found::At(3));
    assert_eq!(forward(src, "abc", 3), Found::At(3));
}

#[test]
fn backward_finds_the_last_match_strictly_before_from() {
    let src = b"abcabc".as_slice();
    assert_eq!(backward(src, "abc", 6), Found::At(3));
    assert_eq!(backward(src, "abc", 4), Found::At(3));
    assert_eq!(backward(src, "abc", 3), Found::At(0));
}

#[test]
fn overlapping_matches_are_found_both_ways() {
    let src = b"aaaa".as_slice();
    assert_eq!(forward(src, "aa", 1), Found::At(1));
    assert_eq!(backward(src, "aa", 3), Found::At(2));
}

#[test]
fn forward_does_not_wrap_around() {
    let src = b"abcabc".as_slice();
    assert_eq!(forward(src, "abc", 4), Found::NotFound);
    assert_eq!(forward(src, "abc", 6), Found::NotFound);
}

#[test]
fn backward_does_not_wrap_around() {
    let src = b"xxabc".as_slice();
    assert_eq!(backward(src, "abc", 2), Found::NotFound);
    assert_eq!(backward(src, "abc", 0), Found::NotFound);
}

#[test]
fn missing_needle_is_not_found() {
    assert_eq!(forward(b"hello".as_slice(), "world", 0), Found::NotFound);
    assert_eq!(backward(b"hello".as_slice(), "world", 5), Found::NotFound);
}

#[test]
fn text_ignores_ascii_case_unless_match_case() {
    let src = b"Hello WORLD".as_slice();
    assert_eq!(run(src, text("world", false), 0, false), Found::At(6));
    assert_eq!(run(src, text("hELLO", false), 11, true), Found::At(0));
    assert_eq!(run(src, text("world", true), 0, false), Found::NotFound);
    assert_eq!(run(src, text("WORLD", true), 0, false), Found::At(6));
}

#[test]
fn text_without_match_case_matches_non_ascii_exactly() {
    let src = "ÉCOLE école".as_bytes();
    assert_eq!(run(src, text("école", false), 0, false), Found::At(7));
    assert_eq!(run(src, text("éCOLE", false), 0, false), Found::At(7));
    assert_eq!(run(src, text("École", false), 3, false), Found::NotFound);
}

#[test]
fn bytes_match_exactly() {
    let src = [0x00, b'M', b'Z', b'm', b'z'];
    assert_eq!(
        run(src.as_slice(), Needle::Bytes(b"MZ".to_vec()), 0, false),
        Found::At(1)
    );
    assert_eq!(
        run(src.as_slice(), Needle::Bytes(b"mz".to_vec()), 0, false),
        Found::At(3)
    );
    assert_eq!(
        run(src.as_slice(), Needle::Bytes(b"Mz".to_vec()), 0, false),
        Found::NotFound
    );
}

#[test]
fn empty_needles_are_not_found() {
    let src = b"abc".as_slice();
    assert_eq!(run(src, text("", false), 0, false), Found::NotFound);
    assert_eq!(run(src, text("", true), 3, true), Found::NotFound);
    assert_eq!(
        run(src, Needle::Bytes(Vec::new()), 0, false),
        Found::NotFound
    );
}

#[test]
fn cancel_already_set_gives_cancelled() {
    let search = Search {
        needle: text("a", false),
        from: 0,
        backward: false,
    };
    let got = find(
        b"abc".as_slice(),
        &search,
        &AtomicBool::new(true),
        &mut |_| {},
    );
    assert_eq!(got.unwrap(), Found::Cancelled);
}

#[test]
fn forward_finds_matches_across_chunk_boundaries() {
    let bytes = big();
    let src = bytes.as_slice();
    assert_eq!(forward(src, "needle", 0), Found::At(FIRST as u64));
    assert_eq!(
        forward(src, "needle", FIRST as u64 + 1),
        Found::At(SECOND as u64)
    );
    assert_eq!(forward(src, "needle", SECOND as u64 + 1), Found::NotFound);
}

#[test]
fn backward_finds_matches_across_chunk_boundaries() {
    let bytes = big();
    let src = bytes.as_slice();
    let len = bytes.len() as u64;
    assert_eq!(backward(src, "needle", len), Found::At(SECOND as u64));
    assert_eq!(
        backward(src, "needle", SECOND as u64),
        Found::At(FIRST as u64)
    );
    assert_eq!(backward(src, "needle", FIRST as u64), Found::NotFound);
}

#[test]
fn search_loops_over_short_reads() {
    let src = Short::new(&big(), 4096 + 7);
    assert_eq!(
        run(&src, text("NEEDLE", false), 0, false),
        Found::At(FIRST as u64)
    );
    let len = src.len();
    assert_eq!(
        run(&src, text("needle", true), len, true),
        Found::At(SECOND as u64)
    );
}

fn progress_calls(backward: bool) -> Vec<u64> {
    let bytes = big();
    let search = Search {
        needle: text("absent", false),
        from: if backward { bytes.len() as u64 } else { 0 },
        backward,
    };
    let mut calls = Vec::new();
    let got = find(
        bytes.as_slice(),
        &search,
        &AtomicBool::new(false),
        &mut |n| {
            calls.push(n);
        },
    );
    assert_eq!(got.unwrap(), Found::NotFound);
    calls
}

#[test]
fn progress_is_reported_per_chunk_forward() {
    let calls = progress_calls(false);
    assert!(calls.len() >= 3, "{calls:?}");
    assert!(calls.windows(2).all(|pair| pair[0] <= pair[1]), "{calls:?}");
}

#[test]
fn progress_is_reported_per_chunk_backward() {
    let calls = progress_calls(true);
    assert!(calls.len() >= 3, "{calls:?}");
    assert!(calls.windows(2).all(|pair| pair[0] <= pair[1]), "{calls:?}");
}
