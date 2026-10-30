use std::io::{self, Read};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusty_core::encoding::{DetectedBy, Encoding};
use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use super::*;
use crate::model::{
    CancelToken, ContentClass, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry,
    SideSource, Stamp,
};

fn encoding(label: &[u8]) -> &'static Encoding {
    Encoding::for_label(label).unwrap()
}

fn utf8() -> &'static Encoding {
    encoding(b"utf-8")
}

fn utf16_le() -> &'static Encoding {
    encoding(b"utf-16le")
}

fn utf16_be() -> &'static Encoding {
    encoding(b"utf-16be")
}

fn fallback() -> &'static Encoding {
    encoding(b"windows-1252")
}

fn windows_1251() -> &'static Encoding {
    encoding(b"windows-1251")
}

struct FakeSource {
    provider: ProviderId,
    bytes: Vec<u8>,
    len: u64,
    reads: Arc<AtomicUsize>,
}

impl FakeSource {
    fn new(bytes: Vec<u8>, len: u64) -> Self {
        Self {
            provider: ProviderId::local(),
            bytes,
            len,
            reads: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn of(bytes: &[u8]) -> Self {
        Self::new(bytes.to_vec(), u64::try_from(bytes.len()).unwrap())
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }
}

struct Counter {
    bytes: Vec<u8>,
    len: u64,
    pos: u64,
    reads: Arc<AtomicUsize>,
}

impl Read for Counter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let start = usize::try_from(self.pos).unwrap_or(usize::MAX);
        let available = self.bytes.len().saturating_sub(start);
        let wanted = usize::try_from(self.len - self.pos).unwrap_or(usize::MAX);
        let count = available.min(wanted).min(buf.len());
        if count == 0 {
            return Ok(0);
        }
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        self.pos += u64::try_from(count).unwrap_or(u64::MAX);
        self.reads.fetch_add(count, Ordering::Relaxed);
        Ok(count)
    }
}

impl SideSource for FakeSource {
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

    fn open(&self, _: &RelPath, _: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        Ok(Box::new(Counter {
            bytes: self.bytes.clone(),
            len: self.len,
            pos: 0,
            reads: self.reads.clone(),
        }))
    }

    fn stamp(&self, _: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.len,
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
        ProviderPath::from_text("fake/file.txt")
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

fn entry(size: u64) -> SideEntry {
    SideEntry::new(
        RelPath::new(ProviderPath::from_text("file.txt")),
        EntryKind::File,
        size,
        None,
    )
}

fn decode_bytes(bytes: &[u8]) -> (TextSide, usize) {
    decode_with(bytes, fallback())
}

fn decode_with(bytes: &[u8], fallback: &'static Encoding) -> (TextSide, usize) {
    let source = FakeSource::of(bytes);
    let size = u64::try_from(bytes.len()).unwrap();
    let side = TextSide::decode(&source, &entry(size), fallback).unwrap();
    (side, source.reads())
}

fn whole(bytes: &[u8]) -> Whole {
    match decode_bytes(bytes).0 {
        TextSide::Whole(whole) => whole,
        other => panic!("expected whole text, got {other:?}"),
    }
}

fn utf16(text: &str, little: bool, bom: bool) -> Vec<u8> {
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

#[test]
fn utf8_without_a_bom_is_whole() {
    let bytes = b"one\ntwo";
    let (side, reads) = decode_bytes(bytes);
    let whole = match side {
        TextSide::Whole(whole) => whole,
        other => panic!("expected whole text, got {other:?}"),
    };

    assert_eq!(whole.layout.text.encoding, utf8());
    assert!(!whole.layout.text.bom);
    assert!(whole.lossless);
    assert_eq!(whole.lines.len(), 2);
    assert_eq!(whole.lines[0].text, "one");
    assert_eq!(whole.lines[0].ending, LineEnding::Lf);
    assert_eq!(whole.lines[1].ending, LineEnding::None);
    assert_eq!(reads, bytes.len());
}

#[test]
fn utf8_with_a_bom_hides_it_from_the_first_line() {
    let bytes = [&[0xEF, 0xBB, 0xBF][..], b"one\ntwo"].concat();
    let found = detect(&bytes, fallback());

    assert_eq!(found.text.encoding, utf8());
    assert!(found.text.bom);
    assert_eq!(found.text.by, DetectedBy::Bom);
    let whole = whole(&bytes);
    assert_eq!(whole.lines.len(), 2);
    assert_eq!(whole.lines[0].text, "one");
    assert!(whole.lines[0].bom);
}

#[test]
fn utf16_le_with_a_bom_decodes() {
    let bytes = utf16("one\r\ntwo", true, true);
    let found = detect(&bytes, fallback());

    assert_eq!(found.text.encoding, utf16_le());
    assert!(found.text.bom);
    let whole = whole(&bytes);
    assert_eq!(whole.lines[0].text, "one");
    assert_eq!(whole.lines[0].ending, LineEnding::CrLf);
    assert_eq!(whole.lines[1].text, "two");
    assert_eq!(content_class(&bytes), ContentClass::Text);
}

#[test]
fn utf16_le_without_a_bom_decodes() {
    let bytes = utf16("one\ntwo", true, false);
    let found = detect(&bytes, fallback());

    assert_eq!(found.text.encoding, utf16_le());
    assert!(!found.text.bom);
    let whole = whole(&bytes);
    assert_eq!(whole.lines[0].text, "one");
    assert_eq!(whole.lines[1].text, "two");
    assert_eq!(content_class(&bytes), ContentClass::Text);
}

#[test]
fn utf16_be_with_a_bom_decodes() {
    let bytes = utf16("one\ntwo", false, true);
    let found = detect(&bytes, fallback());

    assert_eq!(found.text.encoding, utf16_be());
    assert!(found.text.bom);
    let whole = whole(&bytes);
    assert_eq!(whole.lines[0].text, "one");
    assert_eq!(whole.lines[1].text, "two");
}

#[test]
fn utf16_be_without_a_bom_decodes() {
    let bytes = utf16("one\ntwo", false, false);
    let found = detect(&bytes, fallback());

    assert_eq!(found.text.encoding, utf16_be());
    assert!(!found.text.bom);
    let whole = whole(&bytes);
    assert_eq!(whole.lines[0].text, "one");
    assert_eq!(whole.lines[1].text, "two");
}

#[test]
fn mixed_line_endings_are_kept() {
    let whole = whole(b"one\r\ntwo\nthree\r\nfour");
    let endings: Vec<LineEnding> = whole.lines.iter().map(|line| line.ending).collect();
    let texts: Vec<&str> = whole.lines.iter().map(|line| line.text.as_str()).collect();

    assert_eq!(
        endings,
        [
            LineEnding::CrLf,
            LineEnding::Lf,
            LineEnding::CrLf,
            LineEnding::None
        ]
    );
    assert_eq!(texts, ["one", "two", "three", "four"]);
}

#[test]
fn binary_text_is_refused() {
    assert_eq!(content_class(b"abc\0def"), ContentClass::Binary);
    let source = FakeSource::of(b"abc\0def");
    assert_eq!(
        TextSide::decode(&source, &entry(7), fallback()),
        Err(Reason::Unsupported)
    );

    let mut late = vec![b'a'; BINARY_SNIFF];
    late.push(0);
    assert_eq!(content_class(&late), ContentClass::Text);
}

#[test]
fn a_large_side_stops_after_the_sniff() {
    let len = 65 * 1024 * 1024;
    let source = FakeSource::new(vec![b'a'; SNIFF_LEN], len);
    let side = TextSide::decode(&source, &entry(len), fallback()).unwrap();

    let TextSide::Large(large) = side else {
        panic!("expected a large side");
    };
    assert_eq!(large.len, len);
    assert!(large.layout.is_utf8());
    assert_eq!(source.reads(), SNIFF_LEN);
}

#[test]
fn the_caller_fallback_decodes_unknown_bytes() {
    let found = detect(&[0x80], windows_1251());
    assert_eq!(found.text.encoding, windows_1251());
    assert_eq!(found.text.by, DetectedBy::Fallback);

    let (side, _) = decode_with(&[0x80], windows_1251());
    let TextSide::Whole(whole) = side else {
        panic!("expected whole text");
    };
    assert_eq!(whole.lines[0].text, "Ђ");
}

#[test]
fn a_bom_only_file_differs_from_an_empty_file_unless_bom_is_ignored() {
    let bom_only = whole(&[0xEF, 0xBB, 0xBF]);
    let empty = whole(b"");

    assert!(bom_only.lines.is_empty());
    assert!(empty.lines.is_empty());
    assert_ne!(
        bom_only.keys(&Options::default()),
        empty.keys(&Options::default())
    );

    let ignored = Options {
        ignore_bom: true,
        ..Options::default()
    };
    assert_eq!(bom_only.keys(&ignored), empty.keys(&ignored));
}

#[test]
fn a_lone_utf16_surrogate_decodes_with_replacement() {
    let bytes = [b'h', 0, b'e', 0, b'l', 0, b'l', 0, b'o', 0, 0x01, 0xD8];
    let whole = whole(&bytes);

    assert_eq!(whole.layout.text.encoding, utf16_le());
    assert_eq!(whole.lines[0].text, "hello\u{FFFD}");
    assert!(!whole.lossless);
}

#[test]
fn a_multibyte_character_cut_at_the_sniff_boundary_still_detects_utf8() {
    let mut bytes = vec![b'a'; SNIFF_LEN - 1];
    bytes.extend_from_slice("é".as_bytes());
    bytes.extend_from_slice(b"tail");
    let whole = whole(&bytes);

    assert_eq!(whole.layout.text.encoding, utf8());
    assert!(whole.lossless);
    assert!(!whole.lines[0].text.contains('\u{FFFD}'));
    assert!(whole.lines[0].text.ends_with("étail"));
}

#[test]
fn source_starts_count_the_bom_and_stored_endings() {
    let (side, _) = decode_with(&utf16("a\nb\n", true, true), fallback());
    let TextSide::Whole(decoded) = side else {
        panic!("expected whole text");
    };
    assert_eq!(decoded.starts, [2, 6, 10]);
    assert_eq!(decoded.line_set().starts, [2, 6, 10]);

    let utf8_bom = [&[0xEF, 0xBB, 0xBF][..], b"a\nb\n"].concat();
    let decoded = whole(&utf8_bom);
    assert_eq!(decoded.starts, [3, 5, 7]);

    let windows = whole(b"\xE9\nb\n");
    assert_eq!(windows.layout.text.encoding, fallback());
    assert_eq!(windows.starts, [0, 2, 4]);
}

#[test]
fn a_bom_only_side_has_only_its_end_start() {
    let bom_only = whole(&[0xEF, 0xBB, 0xBF]);
    assert!(bom_only.lines.is_empty());
    assert!(bom_only.layout.text.bom);
    assert_eq!(bom_only.starts, [3]);

    let empty = whole(b"");
    assert!(!empty.layout.text.bom);
    assert_eq!(empty.starts, [0]);
}
