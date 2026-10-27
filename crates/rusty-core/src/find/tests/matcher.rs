use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::find::{Compiled, FileHits, FindQuery, Skip, scan};

fn search(bytes: &[u8], text: &str) -> FileHits {
    let compiled = Compiled::new(&FindQuery::parse(text)).unwrap();
    scan(bytes, &compiled, &AtomicBool::new(false)).unwrap()
}

#[test]
fn utf8_hits_have_character_columns_and_count_occurrences() {
    let found = search("é hello hello\nlast hello".as_bytes(), "hello");
    assert_eq!(found.total, 3);
    assert_eq!(found.hits[0].line, 1);
    assert_eq!(found.hits[0].column, 3);
    assert_eq!(found.hits[1].column, 9);
    assert_eq!(found.hits[2].line, 2);
    assert_eq!(found.hits[0].text, "é hello hello");
    assert_eq!(found.skipped, None);
}

#[test]
fn utf16_boms_select_both_byte_orders() {
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in "é\r\nhello 😀".encode_utf16() {
            bytes.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        let found = search(&bytes, "😀");
        assert_eq!(found.total, 1);
        assert_eq!(found.hits[0].line, 2);
        assert_eq!(found.hits[0].column, 7);
        assert_eq!(found.hits[0].text, "hello 😀");
    }
}

#[test]
fn invalid_utf8_lines_use_windows_1252_independently() {
    let found = search(b"\xe9\n\xc3\xa9", "é");
    assert_eq!(found.total, 2);
    assert_eq!(found.hits[0].text, "é");
    assert_eq!(found.hits[0].column, 1);
    assert_eq!(found.hits[1].text, "é");
}

#[test]
fn crlf_and_empty_lines_preserve_line_numbers() {
    let found = search(b"\r\nhello\r\n\r\nhello\n", "hello");
    assert_eq!(found.total, 2);
    assert_eq!(found.hits[0].line, 2);
    assert_eq!(found.hits[1].line, 4);
    assert_eq!(found.hits[0].text, "hello");
}

#[test]
fn partial_lines_and_matches_cross_chunk_boundaries() {
    let mut bytes = vec![b'x'; 8192 + 1024 * 1024 - 2];
    bytes.extend_from_slice(b"needle\nneedle");
    let found = search(&bytes, "needle");
    assert_eq!(found.total, 2);
    assert_eq!(found.hits[0].column, 8192 + 1024 * 1024 - 1);
    assert_eq!(found.hits[1].line, 2);
}

#[test]
fn nul_in_first_eight_kib_skips_binary_only_without_utf16_bom() {
    let mut bytes = vec![b'x'; 8192];
    bytes[8191] = 0;
    assert_eq!(search(&bytes, "x").skipped, Some(Skip::Binary));
    bytes[8191] = b'x';
    bytes.extend_from_slice(b"\0needle");
    assert_eq!(search(&bytes, "needle").total, 1);
}

#[test]
fn regex_and_literal_case_flags_apply() {
    for regex in [false, true] {
        let mut query = FindQuery {
            text: if regex { "h.llo" } else { "hello" }.into(),
            regex,
            ..FindQuery::default()
        };
        for case in [false, true] {
            query.case = case;
            let compiled = Compiled::new(&query).unwrap();
            let found = scan(&b"HELLO hello"[..], &compiled, &AtomicBool::new(false)).unwrap();
            assert_eq!(found.total, if case { 1 } else { 2 });
        }
    }
    assert_eq!(search("É é".as_bytes(), "é").total, 2);
    assert_eq!(search(b"h.llo hello", "h.llo").total, 1);
}

#[test]
fn invalid_regex_returns_an_error() {
    let query = FindQuery {
        text: "[".into(),
        regex: true,
        ..FindQuery::default()
    };
    assert!(Compiled::new(&query).is_err());
}

#[test]
fn keeps_one_thousand_hits_but_counts_all() {
    let found = search("hello\n".repeat(1005).as_bytes(), "hello");
    assert_eq!(found.hits.len(), 1000);
    assert_eq!(found.total, 1005);
}

#[test]
fn snippets_keep_two_hundred_characters_around_match() {
    let text = format!("{}needle{}", "é".repeat(300), "界".repeat(300));
    let found = search(text.as_bytes(), "needle");
    assert_eq!(found.hits[0].text.chars().count(), 200);
    assert!(found.hits[0].text.contains("needle"));
    assert_eq!(found.hits[0].column, 301);
}

#[test]
fn empty_content_has_no_hits() {
    assert_eq!(search(b"anything\n", "ext:rs").total, 0);
    assert_eq!(search(b"", "hello").total, 0);
}

struct CancellingReader<'a> {
    cancel: &'a AtomicBool,
    reads: usize,
}

impl Read for CancellingReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        buffer.fill(b'x');
        self.cancel.store(true, Ordering::Relaxed);
        Ok(buffer.len())
    }
}

#[test]
fn cancellation_stops_before_another_read() {
    let cancel = AtomicBool::new(false);
    let mut reader = CancellingReader {
        cancel: &cancel,
        reads: 0,
    };
    let compiled = Compiled::new(&FindQuery::parse("x")).unwrap();
    let found = scan(&mut reader, &compiled, &cancel).unwrap();
    assert_eq!(reader.reads, 1);
    assert_eq!(found.skipped, Some(Skip::Cancelled));
    let found = scan(&mut reader, &compiled, &cancel).unwrap();
    assert_eq!(reader.reads, 1);
    assert_eq!(found.skipped, Some(Skip::Cancelled));
}

#[test]
fn read_errors_propagate() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken reader"))
        }
    }
    let compiled = Compiled::new(&FindQuery::parse("x")).unwrap();
    assert!(scan(Broken, &compiled, &AtomicBool::new(false)).is_err());
}
