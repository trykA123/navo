use std::sync::atomic::{AtomicBool, Ordering};

use proptest::prelude::*;

use super::tests_support::Short;
use super::{ByteSource, Counted, Needle, SEARCH_CHUNK, count, ordinal};

fn text(text: &str, match_case: bool) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case,
    }
}

fn run<S: ByteSource + ?Sized>(src: &S, needle: &Needle, keep: usize) -> Counted {
    count(src, needle, keep, &AtomicBool::new(false), &mut |_, _| {}).unwrap()
}

fn exact(src: &[u8], needle: &str) -> Counted {
    run(src, &text(needle, true), usize::MAX)
}

fn counted(starts: &[u64]) -> Counted {
    Counted {
        total: starts.len() as u64,
        starts: starts.to_vec(),
        done: true,
    }
}

const SPLIT: usize = SEARCH_CHUNK - 3;

fn three_chunks() -> Vec<u8> {
    let mut bytes = vec![b'.'; 3 * SEARCH_CHUNK];
    for start in [10, SEARCH_CHUNK + 10, 2 * SEARCH_CHUNK + 10] {
        bytes[start..start + 6].copy_from_slice(b"needle");
    }
    bytes
}

#[test]
fn count_gives_the_total_and_every_start() {
    let got = exact(b"one two one three one".as_slice(), "one");
    assert_eq!(got, counted(&[0, 8, 18]));
}

#[test]
fn count_of_an_absent_needle_is_zero_and_done() {
    let got = exact(b"one two".as_slice(), "four");
    assert_eq!(got, counted(&[]));
}

#[test]
fn count_matches_do_not_overlap() {
    let got = exact(b"aaaa".as_slice(), "aa");
    assert_eq!(got, counted(&[0, 2]));
}

#[test]
fn count_matches_do_not_overlap_in_an_odd_run() {
    let got = exact(b"aaaaa".as_slice(), "aa");
    assert_eq!(got, counted(&[0, 2]));
}

#[test]
fn count_finds_a_match_across_the_chunk_boundary_once() {
    let mut bytes = vec![b'.'; SEARCH_CHUNK + 100];
    bytes[SPLIT..SPLIT + 6].copy_from_slice(b"needle");
    let src = Short::new(&bytes, 4096 + 7);
    let got = run(&src, &text("needle", true), usize::MAX);
    assert_eq!(got, counted(&[SPLIT as u64]));
}

#[test]
fn count_keeps_non_overlapping_parity_across_chunks() {
    let bytes = vec![b'a'; 2 * SEARCH_CHUNK + 3];
    let got = exact(&bytes, "aa");
    assert_eq!(got.total, SEARCH_CHUNK as u64 + 1);
    assert_eq!(got.starts.len(), SEARCH_CHUNK + 1);
    assert_eq!(got.starts.last(), Some(&(2 * SEARCH_CHUNK as u64)));
    assert!(got.starts.windows(2).all(|pair| pair[1] == pair[0] + 2));
    assert!(got.done);
}

#[test]
fn count_folds_ascii_case_without_match_case() {
    let src = b"Word WORD word wOrD".as_slice();
    let got = run(src, &text("word", false), usize::MAX);
    assert_eq!(got, counted(&[0, 5, 10, 15]));
}

#[test]
fn count_respects_case_with_match_case() {
    let src = b"Word WORD word wOrD".as_slice();
    let got = run(src, &text("WORD", true), usize::MAX);
    assert_eq!(got, counted(&[5]));
}

#[test]
fn count_matches_hex_bytes_exactly() {
    let src = [0x40, 0x00, 0x40, 0x41, 0x40, 0x00, 0x60, 0x00];
    let got = run(src.as_slice(), &Needle::Bytes(vec![0x40, 0x00]), usize::MAX);
    assert_eq!(got, counted(&[0, 4]));
}

#[test]
fn count_keep_caps_the_starts_but_not_the_total() {
    let got = run(b"abababab".as_slice(), &text("ab", true), 2);
    let want = Counted {
        total: 4,
        starts: vec![0, 2],
        done: true,
    };
    assert_eq!(got, want);
}

#[test]
fn count_keep_zero_still_counts() {
    let got = run(b"abab".as_slice(), &text("ab", true), 0);
    let want = Counted {
        total: 2,
        starts: Vec::new(),
        done: true,
    };
    assert_eq!(got, want);
}

#[test]
fn count_cancelled_mid_way_returns_the_counts_so_far() {
    let bytes = three_chunks();
    let cancel = AtomicBool::new(false);
    let got = count(
        bytes.as_slice(),
        &text("needle", true),
        usize::MAX,
        &cancel,
        &mut |_, _| cancel.store(true, Ordering::Relaxed),
    )
    .unwrap();
    assert!(!got.done, "{got:?}");
    assert!((1..3).contains(&got.total), "{got:?}");
    assert_eq!(got.starts.len() as u64, got.total);
}

#[test]
fn count_of_an_empty_needle_is_zero_and_done() {
    let src = b"abc".as_slice();
    assert_eq!(exact(src, ""), counted(&[]));
    assert_eq!(run(src, &text("", false), 5), counted(&[]));
    assert_eq!(run(src, &Needle::Bytes(Vec::new()), 5), counted(&[]));
}

#[test]
fn count_progress_is_called_and_monotonic() {
    let bytes = three_chunks();
    let mut calls = Vec::new();
    let got = count(
        bytes.as_slice(),
        &text("needle", true),
        usize::MAX,
        &AtomicBool::new(false),
        &mut |scanned, found| calls.push((scanned, found)),
    )
    .unwrap();
    assert_eq!(got.total, 3);
    assert!(calls.len() >= 2, "{calls:?}");
    assert!(
        calls
            .windows(2)
            .all(|pair| pair[0].0 <= pair[1].0 && pair[0].1 <= pair[1].1),
        "{calls:?}"
    );
    assert!(calls.iter().all(|call| call.1 <= got.total), "{calls:?}");
}

#[test]
fn ordinal_of_the_first_start_is_one() {
    assert_eq!(ordinal(&counted(&[3, 9, 20]), 3), Some(1));
}

#[test]
fn ordinal_of_a_middle_start() {
    assert_eq!(ordinal(&counted(&[3, 9, 20]), 9), Some(2));
}

#[test]
fn ordinal_of_the_last_start() {
    assert_eq!(ordinal(&counted(&[3, 9, 20]), 20), Some(3));
}

#[test]
fn ordinal_of_a_non_start_is_none() {
    let found = counted(&[3, 9, 20]);
    assert_eq!(ordinal(&found, 9), Some(2));
    assert_eq!(ordinal(&found, 10), None);
}

#[test]
fn ordinal_of_a_start_beyond_keep_is_none() {
    let found = run(b"ababab".as_slice(), &text("ab", true), 2);
    assert_eq!(ordinal(&found, 2), Some(2));
    assert_eq!(ordinal(&found, 4), None);
}

fn naive(hay: &[u8], needle: &[u8], match_case: bool) -> Vec<u64> {
    let fold = |byte: &u8| {
        if match_case {
            *byte
        } else {
            byte.to_ascii_lowercase()
        }
    };
    let hay: Vec<u8> = hay.iter().map(fold).collect();
    let needle: Vec<u8> = needle.iter().map(fold).collect();
    let mut starts = Vec::new();
    let mut at = 0;
    while at + needle.len() <= hay.len() {
        if hay[at..at + needle.len()] == needle[..] {
            starts.push(at as u64);
            at += needle.len();
        } else {
            at += 1;
        }
    }
    starts
}

proptest! {
    #[test]
    fn count_agrees_with_a_naive_scan(
        hay in "[abA]{0,64}",
        needle in "[abA]{1,3}",
        match_case in any::<bool>(),
    ) {
        let got = run(hay.as_bytes(), &text(&needle, match_case), usize::MAX);
        let starts = naive(hay.as_bytes(), needle.as_bytes(), match_case);
        prop_assert_eq!(got, counted(&starts));
    }
}
