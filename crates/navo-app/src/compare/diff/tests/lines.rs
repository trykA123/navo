//! Tests for the large view's line reader: long lines, cut characters, uneven sides,
//! changed sources and UTF-16 layouts.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use navo_compare::model::{CancelToken, Reason};
use navo_compare::text::large::LargeSide;
use navo_core::encoding::{DetectedBy, TextEncoding};
use navo_core::paging::{ByteSource, TextLayout};

use crate::compare::diff::large::lines::Reader;

const MIB: usize = 1024 * 1024;

/// An in-memory source whose reported length the test can shrink; its layout is the default.
/// `check` is what `revalidate` answers; `None` is a failed check.
struct Mem {
    bytes: Vec<u8>,
    len: AtomicU64,
    check: Option<bool>,
}

impl ByteSource for Mem {
    fn len(&self) -> u64 {
        self.len.load(Ordering::Relaxed)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let end = usize::try_from(self.len()).unwrap();
        let from = usize::try_from(offset).unwrap().min(end);
        let count = buf.len().min(end - from);
        buf[..count].copy_from_slice(&self.bytes[from..from + count]);
        Ok(count)
    }

    fn revalidate(&self) -> std::io::Result<bool> {
        self.check
            .ok_or_else(|| std::io::Error::other("stat failed"))
    }
}

fn mem(bytes: Vec<u8>) -> Arc<Mem> {
    let len = AtomicU64::new(bytes.len() as u64);
    Arc::new(Mem {
        bytes,
        len,
        check: Some(true),
    })
}

/// A side whose fresh stamp check answers `check`.
fn checked(text: &str, check: Option<bool>) -> LargeSide {
    let bytes = text.as_bytes().to_vec();
    let len = AtomicU64::new(bytes.len() as u64);
    let source = Arc::new(Mem { bytes, len, check });
    side(source, TextLayout::default())
}

/// UTF-16 LE bytes of `text`, without a BOM.
fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16_le() -> TextLayout {
    TextLayout::of(TextEncoding {
        encoding: encoding_rs::UTF_16LE,
        bom: false,
        by: DetectedBy::Chosen,
    })
}

fn side(source: Arc<Mem>, layout: TextLayout) -> LargeSide {
    LargeSide { source, layout }
}

fn utf8(text: &str) -> LargeSide {
    side(mem(text.as_bytes().to_vec()), TextLayout::default())
}

#[test]
fn a_line_over_the_side_limit_is_marked_cut() {
    let long = format!("{}\nnext\n", "a".repeat(3 * MIB));
    let mut reader = Reader::new(utf8(&long), utf8("short\nnext\n"));
    let window = reader.read(0..2, &CancelToken::new()).unwrap();
    assert_eq!(window.rows, 0..1, "the window stops after the long line");
    assert_eq!(
        window.left[0].len(),
        2 * MIB,
        "the long line is read up to the limit"
    );
    assert!(window.left_cut, "the cut line says so");
    assert!(!window.right_cut, "a whole line is not cut");
    assert_eq!(window.right, ["short"]);
}

#[test]
fn rows_past_the_shorter_side_keep_the_longer_side() {
    let mut reader = Reader::new(utf8("l0\nl1\nl2\nl3\nl4\n"), utf8("r0\nr1\n"));
    let window = reader.read(3..5, &CancelToken::new()).unwrap();
    assert_eq!(window.rows, 3..5);
    assert_eq!(window.left, ["l3", "l4"]);
    assert!(window.right.is_empty(), "an exhausted side reads as empty");
    assert!(!window.left_cut && !window.right_cut);
}

#[test]
fn a_side_that_shrank_after_indexing_reports_changed_on_disk() {
    let right = mem(b"r0\nr1\nr2\nr3\nr4\n".to_vec());
    let mut reader = Reader::new(
        utf8("l0\nl1\nl2\nl3\nl4\n"),
        side(Arc::clone(&right), TextLayout::default()),
    );
    reader.read(0..1, &CancelToken::new()).unwrap();
    right.len.store(6, Ordering::Relaxed);
    let result = reader.read(3..5, &CancelToken::new());
    assert!(matches!(result, Err(Reason::ChangedOnDisk)));
}

#[test]
fn a_utf16_le_window_splits_on_wide_newlines() {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "alpha\nbeta\ngamma\ndelta\n".encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    let layout = TextLayout::of(TextEncoding {
        encoding: encoding_rs::UTF_16LE,
        bom: true,
        by: DetectedBy::Bom,
    });
    let left = side(mem(bytes.clone()), layout);
    let mut reader = Reader::new(left, side(mem(bytes), layout));
    let window = reader.read(1..3, &CancelToken::new()).unwrap();
    assert_eq!(window.rows, 1..3);
    assert_eq!(window.left, ["beta", "gamma"]);
    assert_eq!(window.right, ["beta", "gamma"]);
}

#[test]
fn a_cut_never_splits_a_utf8_character() {
    let long = format!("{}\u{20ac}\n", "a".repeat(2 * MIB - 1));
    let mut reader = Reader::new(utf8(&long), utf8("short\n"));
    let window = reader.read(0..1, &CancelToken::new()).unwrap();
    assert!(window.left_cut, "the line is still marked cut");
    assert!(!window.left[0].contains('\u{fffd}'), "no broken character");
    assert_eq!(
        window.left[0].len(),
        2 * MIB - 1,
        "the cut steps back before the euro sign"
    );
}

#[test]
fn a_cut_never_splits_a_utf16_surrogate_pair() {
    let text = format!("{}\u{1f600}\n", "a".repeat(MIB - 1));
    let left = side(mem(wide(&text)), utf16_le());
    let right = side(mem(wide("short\n")), utf16_le());
    let mut reader = Reader::new(left, right);
    let window = reader.read(0..1, &CancelToken::new()).unwrap();
    assert!(window.left_cut);
    assert!(!window.left[0].contains('\u{fffd}'), "no lone surrogate");
    assert_eq!(
        window.left[0].len(),
        MIB - 1,
        "the cut steps back before the pair"
    );
}

#[test]
fn a_window_across_the_shorter_end_keeps_both_sides_aligned() {
    let mut reader = Reader::new(utf8("l0\nl1\nl2\nl3\nl4\n"), utf8("r0\nr1\n"));
    let window = reader.read(1..4, &CancelToken::new()).unwrap();
    assert_eq!(window.rows, 1..4);
    assert_eq!(window.left, ["l1", "l2", "l3"]);
    assert_eq!(window.right, ["r1"], "the shorter side stops at its end");
}

#[test]
fn a_failed_stamp_check_past_the_end_reports_changed_on_disk() {
    let short = checked("r0\nr1\n", Some(false));
    let mut reader = Reader::new(utf8("l0\nl1\nl2\nl3\nl4\n"), short);
    let result = reader.read(3..5, &CancelToken::new());
    assert!(matches!(result, Err(Reason::ChangedOnDisk)));
}

#[test]
fn a_stamp_check_error_past_the_end_fails_closed() {
    let short = checked("r0\nr1\n", None);
    let mut reader = Reader::new(utf8("l0\nl1\nl2\nl3\nl4\n"), short);
    let result = reader.read(3..5, &CancelToken::new());
    assert!(matches!(result, Err(Reason::ReadError)));
}
