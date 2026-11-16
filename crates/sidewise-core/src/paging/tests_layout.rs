use std::sync::atomic::AtomicBool;

use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::tests_support::Short;
use super::{
    ByteSource, Found, Laid, LineIndex, MAX_ROW, Needle, Search, TextLayout, count, find,
    previous_row, row_at, row_start_at, rows_down, rows_up,
};
use crate::encoding::{DetectedBy, TextEncoding};

fn layout(encoding: &'static encoding_rs::Encoding, bom: bool) -> TextLayout {
    TextLayout::of(TextEncoding {
        encoding,
        bom,
        by: DetectedBy::Chosen,
    })
}

fn wide(text: &str, little: bool, bom: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if bom {
        bytes.extend(if little { [0xFF, 0xFE] } else { [0xFE, 0xFF] });
    }
    for unit in text.encode_utf16() {
        bytes.extend(if little {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        });
    }
    bytes
}

fn laid(bytes: &[u8], layout: TextLayout) -> Laid<Short> {
    Laid::new(Short::new(bytes, usize::MAX), layout)
}

fn rows_of<S: ByteSource + ?Sized>(src: &S) -> Vec<(u64, String)> {
    let layout = src.layout();
    let skip = layout.bom_len() as u64;
    let mut rows = Vec::new();
    let mut at = 0;
    while at < src.len() {
        let row = row_at(src, at).unwrap();
        let start = if at == 0 { skip.min(row.end) } else { at };
        let mut bytes = vec![0u8; usize::try_from(row.end - start).unwrap()];
        super::read_full(src, start, &mut bytes).unwrap();
        rows.push((row.start, layout.decode(&bytes)));
        at = row.next;
    }
    rows
}

#[test]
fn utf16_rows_end_at_the_wide_newline_in_both_byte_orders() {
    for little in [true, false] {
        let bytes = wide("one\r\ntwo\nthree", little, true);
        let src = laid(
            &bytes,
            layout(if little { UTF_16LE } else { UTF_16BE }, true),
        );
        let rows: Vec<String> = rows_of(&src).into_iter().map(|(_, text)| text).collect();
        assert_eq!(rows, ["one", "two", "three"], "little {little}");
    }
}

#[test]
fn a_newline_byte_inside_another_character_does_not_end_a_row() {
    let text = "a\u{0A41}b\u{410A}c\nd";
    for little in [true, false] {
        let bytes = wide(text, little, false);
        let src = laid(
            &bytes,
            layout(if little { UTF_16LE } else { UTF_16BE }, false),
        );
        let rows: Vec<String> = rows_of(&src).into_iter().map(|(_, text)| text).collect();
        assert_eq!(rows, ["a\u{0A41}b\u{410A}c", "d"], "little {little}");
    }
}

#[test]
fn rows_found_backwards_match_rows_found_forwards_in_utf16() {
    let text: String = (0..300)
        .map(|n| format!("line {n} \u{1F600} d\u{E9}j\u{E0}\r\n"))
        .chain(["x".repeat(9000), "\n".to_owned(), "tail".to_owned()])
        .collect();
    let bytes = wide(&text, true, true);
    let src = laid(&bytes, layout(UTF_16LE, true));
    let starts: Vec<u64> = rows_of(&src).into_iter().map(|(start, _)| start).collect();
    for pair in starts.windows(2) {
        assert_eq!(previous_row(&src, pair[1]).unwrap(), pair[0]);
        assert_eq!(row_start_at(&src, pair[0] + 3).unwrap(), pair[0]);
    }
    let last = starts[starts.len() - 1];
    assert_eq!(rows_up(&src, last, 2).unwrap(), starts[starts.len() - 3]);
    assert_eq!(rows_down(&src, starts[1], 2).unwrap(), starts[3]);
}

#[test]
fn a_long_utf16_line_is_cut_between_characters() {
    let text = format!("a{}", "\u{1F600}".repeat(MAX_ROW as usize));
    let bytes = wide(&text, false, false);
    let src = laid(&bytes, layout(UTF_16BE, false));
    for (_, row) in rows_of(&src) {
        assert!(!row.contains('\u{FFFD}'), "a surrogate pair was split");
    }
}

#[test]
fn the_line_index_counts_utf16_lines_and_finds_their_starts() {
    let text = "alpha\r\nbeta\r\n\u{0A41}gamma\r\nlast";
    for little in [true, false] {
        let bytes = wide(text, little, true);
        let src = laid(
            &bytes,
            layout(if little { UTF_16LE } else { UTF_16BE }, true),
        );
        let mut index = LineIndex::with_layout(src.layout());
        index.feed(&bytes);
        index.finish();
        assert_eq!(index.newlines(), 3, "little {little}");
        assert_eq!(index.line_count(&src).unwrap(), 4);
        let starts: Vec<u64> = rows_of(&src).into_iter().map(|(start, _)| start).collect();
        for (number, start) in starts.iter().enumerate() {
            let line = number as u64 + 1;
            assert_eq!(index.line_start(&src, line).unwrap(), Some(*start));
            assert_eq!(index.line_number(&src, *start).unwrap(), Some(line));
        }
    }
}

fn find_in(src: &Laid<Short>, text: &str, match_case: bool, from: (u64, bool)) -> Found {
    let search = Search {
        needle: Needle::Text {
            text: text.to_owned(),
            match_case,
        },
        from: from.0,
        backward: from.1,
    };
    find(src, &search, &AtomicBool::new(false), &mut |_| {}).unwrap()
}

#[test]
fn searching_utf16_matches_whole_characters_only() {
    let text = "xx NEEDLE yy needle \u{6161}\u{6200}\u{7800}ab";
    let bytes = wide(text, true, false);
    let src = laid(&bytes, layout(UTF_16LE, false));
    assert_eq!(find_in(&src, "needle", false, (0, false)), Found::At(6));
    assert_eq!(find_in(&src, "needle", true, (0, false)), Found::At(26));
    assert_eq!(find_in(&src, "needle", false, (25, true)), Found::At(6));
    assert_eq!(find_in(&src, "ab", true, (0, false)), Found::At(46));
}

#[test]
fn counting_utf16_matches_does_not_double_count() {
    let bytes = wide("aaa aa aa", false, false);
    let src = laid(&bytes, layout(UTF_16BE, false));
    let needle = Needle::Text {
        text: "aa".to_owned(),
        match_case: true,
    };
    let counted = count(&src, &needle, 10, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
    assert_eq!(counted.starts, [0, 8, 14]);
}

#[test]
fn a_single_byte_file_searches_with_the_needle_in_its_encoding() {
    let (bytes, _, _) = WINDOWS_1252.encode("un d\u{E9}j\u{E0} noir\nd\u{E9}j\u{E0}\n");
    let src = laid(&bytes, layout(WINDOWS_1252, false));
    assert_eq!(
        find_in(&src, "d\u{E9}j\u{E0}", true, (0, false)),
        Found::At(3)
    );
    assert_eq!(find_in(&src, "\u{65E5}", true, (0, false)), Found::NotFound);
}

#[test]
fn the_layout_decodes_rows_and_maps_text_to_bytes() {
    let little = layout(UTF_16LE, true);
    assert_eq!(little.bom_len(), 2);
    assert_eq!(
        little.decode(&wide("h\u{E9}\u{1F600}", true, false)),
        "h\u{E9}\u{1F600}"
    );
    assert_eq!(little.raw_len('\u{1F600}'), 4);
    let latin = layout(WINDOWS_1252, false);
    assert_eq!(latin.decode(&[0x63, 0xE9]), "c\u{E9}");
    assert_eq!(latin.raw_len('\u{E9}'), 1);
    assert_eq!(latin.text_range(&[0x63, 0xE9, 0x61], 1, 1), 1..3);
    assert_eq!(layout(UTF_8, false).text_range(b"abc", 1, 1), 1..2);
}

#[test]
fn a_cjk_search_is_case_exact_in_utf16() {
    let text = "x \u{4100} y \u{6100} z";
    for little in [true, false] {
        let encoding = if little { UTF_16LE } else { UTF_16BE };
        let bytes = wide(text, little, false);
        let src = laid(&bytes, layout(encoding, false));
        assert_eq!(
            find_in(&src, "\u{4100}", false, (0, false)),
            Found::At(4),
            "little {little}"
        );
        assert_eq!(
            find_in(&src, "\u{6100}", false, (0, false)),
            Found::At(12),
            "little {little}"
        );
        assert_eq!(
            find_in(&src, "\u{4100}", false, (6, false)),
            Found::NotFound,
            "little {little}"
        );
    }
}

#[test]
fn an_ascii_search_still_folds_case_in_utf16() {
    let bytes = wide("a \u{6100} B", true, false);
    let src = laid(&bytes, layout(UTF_16LE, false));
    assert_eq!(find_in(&src, "b", false, (0, false)), Found::At(8));
    assert_eq!(find_in(&src, "A", false, (0, false)), Found::At(0));
    assert_eq!(find_in(&src, "a", false, (2, false)), Found::NotFound);
}

#[test]
fn only_the_legacy_multi_byte_encodings_need_the_whole_file() {
    for label in ["Shift_JIS", "GBK", "Big5", "EUC-JP", "EUC-KR"] {
        let encoding = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        assert!(layout(encoding, false).needs_whole_file(), "{label}");
    }
    for encoding in [UTF_8, UTF_16LE, UTF_16BE, WINDOWS_1252] {
        assert!(!layout(encoding, false).needs_whole_file());
    }
}
