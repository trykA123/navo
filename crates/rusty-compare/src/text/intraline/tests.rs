use std::ops::ControlFlow;

use rusty_core::encoding::{DetectedBy, Encoding, TextEncoding};
use rusty_core::paging::TextLayout;
use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use super::*;
use crate::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use crate::text::side::{LineEnding, TextSide};

fn line(text: &str) -> Line {
    Line {
        text: text.to_owned(),
        ending: LineEnding::None,
        bom: false,
    }
}

fn layout(label: &[u8]) -> TextLayout {
    let encoding = Encoding::for_label(label).unwrap();
    TextLayout::of(TextEncoding {
        encoding,
        bom: false,
        by: DetectedBy::Chosen,
    })
}

fn side<'a>(line: &'a Line, start: u64, layout: TextLayout) -> SideLine<'a> {
    SideLine {
        line,
        start,
        layout,
        lossless: true,
    }
}

fn pair<'a>(left: &'a Line, right: &'a Line) -> LinePair<'a> {
    LinePair {
        left: side(left, 0, TextLayout::default()),
        right: side(right, 0, TextLayout::default()),
    }
}

fn text_of(line: &Line, chars: Range<usize>) -> String {
    line.text
        .chars()
        .skip(chars.start)
        .take(chars.end - chars.start)
        .collect()
}

#[test]
fn word_spans_mark_changed_words_and_utf8_source_bytes() {
    let left = line("let alpha = 1;");
    let right = line("let beta = 22;");
    let result = intraline(LinePair {
        left: side(&left, 100, TextLayout::default()),
        right: side(&right, 200, TextLayout::default()),
    });

    assert_eq!(result.fallback, None);
    assert!(!result.words.is_empty());
    assert!(!result.chars.is_empty());
    for span in &result.words {
        let line = if span.side == SpanSide::Left {
            &left
        } else {
            &right
        };
        let start = if span.side == SpanSide::Left {
            100
        } else {
            200
        };
        let chars = span.chars.start as u64..span.chars.end as u64;
        assert_eq!(span.source, start + chars.start..start + chars.end);
        assert!(!text_of(line, span.chars.clone()).is_empty());
    }
    assert!(result.words.iter().any(|span| span.side == SpanSide::Left));
    assert!(result.words.iter().any(|span| span.side == SpanSide::Right));
}

#[test]
fn short_lines_get_character_spans() {
    let left = line("abcde");
    let right = line("abXde");
    let result = intraline(pair(&left, &right));

    assert_eq!(result.fallback, None);
    assert!(!result.words.is_empty());
    assert_eq!(result.chars.len(), 2);
    assert!(result.chars.iter().any(|span| {
        span.side == SpanSide::Left && span.chars == (2..3) && span.source == (2..3)
    }));
    assert!(result.chars.iter().any(|span| {
        span.side == SpanSide::Right && span.chars == (2..3) && span.source == (2..3)
    }));
}

#[test]
fn long_but_not_huge_lines_keep_word_spans_only() {
    let left = line(&format!("{} alpha", "x".repeat(100)));
    let right = line(&format!("{} beta", "x".repeat(100)));
    let result = intraline(pair(&left, &right));

    assert!(!result.words.is_empty());
    assert!(result.chars.is_empty());
    assert_eq!(result.fallback, None);
}

#[test]
fn lines_over_the_character_limit_are_whole_line() {
    let left = line(&"a".repeat(INTRALINE_MAX_CHARS + 1));
    let right = line(&"b".repeat(INTRALINE_MAX_CHARS + 1));
    let result = intraline(pair(&left, &right));

    assert!(result.words.is_empty());
    assert!(result.chars.is_empty());
    assert_eq!(result.fallback, Some(IntralineFallback::LineTooLong));
}

#[test]
fn source_offsets_follow_the_side_encoding() {
    let utf8_line = line("abc");
    let utf8 = side(&utf8_line, 5, TextLayout::default());
    assert_eq!(utf8.source_range(1..2), 6..7);

    let unicode_line = line("aé");
    let unicode = side(&unicode_line, 5, TextLayout::default());
    assert_eq!(unicode.source_range(1..2), 6..8);

    let wide_line = line("abc");
    let wide = side(&wide_line, 2, layout(b"utf-16le"));
    assert_eq!(wide.source_range(1..2), 4..6);

    let single_line = line("é");
    let single = side(&single_line, 10, layout(b"windows-1252"));
    assert_eq!(single.source_range(0..1), 10..11);
}

#[test]
fn a_pathological_pair_hits_the_deadline() {
    let left_text = "a ".repeat(2_048);
    let right_text: String = (0..2_048)
        .map(|index| if index % 32 == 0 { "b " } else { "a " })
        .collect();
    let left = line(&left_text);
    let right = line(&right_text);
    let result = intraline_with_deadline(pair(&left, &right), Duration::ZERO);

    assert!(result.words.is_empty());
    assert!(result.chars.is_empty());
    assert_eq!(result.fallback, Some(IntralineFallback::Deadline));
}

struct BytesSource {
    bytes: Vec<u8>,
    provider: ProviderId,
}

impl BytesSource {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            provider: ProviderId::local(),
        }
    }
}

impl SideSource for BytesSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _: &RelPath,
        _: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _: &CancelToken,
    ) -> Result<(), Reason> {
        Ok(())
    }

    fn open(&self, _: &RelPath, _: ReadHint) -> Result<Box<dyn std::io::Read + Send>, Reason> {
        Ok(Box::new(std::io::Cursor::new(self.bytes.clone())))
    }

    fn stamp(&self, _: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.bytes.len() as u64,
            ..Stamp::default()
        })
    }

    fn object_id(&self, _: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, _: &RelPath) -> ProviderPath {
        ProviderPath::from_text("lossy.txt")
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

#[test]
fn a_lossily_decoded_line_is_whole_line() {
    let source = BytesSource::new(&[0xEF, 0xBB, 0xBF, 0xFF, 0x61]);
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("lossy.txt")),
        EntryKind::File,
        source.bytes.len() as u64,
        None,
    );
    let fallback = Encoding::for_label(b"windows-1252").unwrap();
    let TextSide::Whole(whole) = TextSide::decode(&source, &entry, fallback).unwrap() else {
        panic!("expected whole text");
    };
    assert!(!whole.lossless);
    let left_line = &whole.lines[0];
    let right_line = line("a");
    let result = intraline(LinePair {
        left: SideLine {
            line: left_line,
            start: whole.starts[0],
            layout: whole.layout,
            lossless: whole.lossless,
        },
        right: SideLine {
            line: &right_line,
            start: 0,
            layout: TextLayout::default(),
            lossless: true,
        },
    });

    assert!(result.words.is_empty());
    assert!(result.chars.is_empty());
    assert_eq!(result.fallback, Some(IntralineFallback::Lossy));
}
