use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusty_core::encoding::Encoding;
use rusty_core::paging::{ByteSource, TextLayout};
use xxhash_rust::xxh3::xxh3_64;

use super::*;
use crate::model::{CancelToken, CaseRule, Reason};
use crate::text::normalise::{Options, Whitespace, normalise};
use crate::text::side::{Line, LineEnding, Whole, detect};

fn fallback() -> &'static Encoding {
    Encoding::for_label(b"windows-1252").unwrap()
}

fn layout_of(bytes: &[u8]) -> TextLayout {
    detect(bytes, fallback())
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

fn whole(layout: TextLayout, lines: &[(&str, LineEnding)]) -> Whole {
    Whole {
        layout,
        lines: lines
            .iter()
            .enumerate()
            .map(|(index, (text, ending))| Line {
                text: (*text).to_owned(),
                ending: *ending,
                bom: layout.text.bom && index == 0,
            })
            .collect(),
        starts: Vec::new(),
        lossless: true,
    }
}

fn hashes(keys: &[String]) -> Vec<u64> {
    keys.iter().map(|key| xxh3_64(key.as_bytes())).collect()
}

fn candidate_hashes(scanned: &ScanResult) -> Vec<u64> {
    scanned
        .candidates
        .iter()
        .map(|candidate| candidate.hash)
        .collect()
}

fn candidate_lines(scanned: &ScanResult) -> Vec<u32> {
    scanned
        .candidates
        .iter()
        .map(|candidate| candidate.line)
        .collect()
}

fn scan_bytes(bytes: &[u8], layout: TextLayout, options: &Options, rate: u32) -> ScanResult {
    scan(
        bytes,
        layout,
        options,
        rate,
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap()
}

const DEFAULT: Options = Options {
    whitespace: Whitespace::Keep,
    ignore_line_endings: false,
    case: CaseRule::Sensitive,
    ignore_bom: false,
};

fn normalise_source() -> (Vec<u8>, Whole) {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"  Alpha   beta\t\r\n");
    bytes.extend_from_slice(b"alpha beta\n");
    bytes.extend_from_slice("Stra\u{df}e value\r\n".as_bytes());
    bytes.extend_from_slice(b"STRASSE value");
    let layout = layout_of(&bytes);
    let expected = whole(
        layout,
        &[
            ("  Alpha   beta\t", LineEnding::CrLf),
            ("alpha beta", LineEnding::Lf),
            ("Stra\u{df}e value", LineEnding::CrLf),
            ("STRASSE value", LineEnding::None),
        ],
    );
    (bytes, expected)
}

fn normalise_cases() -> [(&'static str, Options); 7] {
    let mut all = DEFAULT;
    all.whitespace = Whitespace::All;
    let mut changes = DEFAULT;
    changes.whitespace = Whitespace::Changes;
    let mut trailing = DEFAULT;
    trailing.whitespace = Whitespace::Trailing;
    let mut endings = DEFAULT;
    endings.ignore_line_endings = true;
    let mut case = DEFAULT;
    case.case = CaseRule::Insensitive;
    let mut bom = DEFAULT;
    bom.ignore_bom = true;
    [
        ("default", DEFAULT),
        ("whitespace all", all),
        ("whitespace changes", changes),
        ("whitespace trailing", trailing),
        ("line endings", endings),
        ("case", case),
        ("bom", bom),
    ]
}

fn assert_matches_e7(bytes: &[u8], text: &str, layout: TextLayout, ending: LineEnding) {
    let expected = whole(layout, &[(text, ending)]);
    for (name, options) in normalise_cases() {
        let scanned = scan_bytes(bytes, layout, &options, 1);
        assert_eq!(scanned.lines, 1, "{name}");
        assert_eq!(
            candidate_hashes(&scanned),
            hashes(&expected.keys(&options)),
            "{name}"
        );
    }
}

#[test]
fn a_line_split_mid_code_unit_matches_e7() {
    let mut text = "a".repeat(BLOCK - 1);
    text.push('\u{e9}');
    let mut bytes = text.as_bytes().to_vec();
    bytes.extend_from_slice(b"\r\n");
    assert_matches_e7(&bytes, &text, TextLayout::default(), LineEnding::CrLf);
}

#[test]
fn a_line_split_mid_whitespace_run_matches_e7() {
    let mut text = "a".repeat(BLOCK - 1);
    text.push(' ');
    text.push_str(&"\t".repeat(4096));
    text.push_str("tail");
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(b'\n');
    assert_matches_e7(&bytes, &text, TextLayout::default(), LineEnding::Lf);
}

#[test]
fn utf16_le_lines_are_decoded_before_hashing() {
    let bytes = wide("alpha one\r\nbeta two ok\ncharlie three", true, true);
    let layout = layout_of(&bytes);
    let options = Options::default();
    let scanned = scan_bytes(&bytes, layout, &options, 1);
    let expected = whole(
        layout,
        &[
            ("alpha one", LineEnding::CrLf),
            ("beta two ok", LineEnding::Lf),
            ("charlie three", LineEnding::None),
        ],
    );

    assert!(layout.is_wide());
    assert!(layout.text.bom);
    assert_eq!(scanned.lines, 3);
    assert_eq!(scanned.index.newlines(), 2);
    assert_eq!(scanned.index.scanned(), bytes.len() as u64);
    assert_eq!(candidate_lines(&scanned), [0, 1, 2]);
    assert_eq!(candidate_hashes(&scanned), hashes(&expected.keys(&options)));
}

#[test]
fn utf16_surrogate_split_across_blocks_matches_e7() {
    let mut text = "a".repeat((BLOCK - 4) / 2);
    text.push('\u{1f600}');
    let mut bytes = wide(&text, true, true);
    bytes.extend_from_slice(&[b'\n', 0]);
    let layout = layout_of(&bytes);
    assert!(layout.is_wide());
    assert_matches_e7(&bytes, &text, layout, LineEnding::Lf);
}

#[test]
fn crlf_and_mixed_endings_stay_in_the_key() {
    let bytes = b"alpha one\r\nbeta two ok\ncharlie three\r\ndelta four";
    let options = Options::default();
    let scanned = scan_bytes(bytes, TextLayout::default(), &options, 1);
    let expected = whole(
        TextLayout::default(),
        &[
            ("alpha one", LineEnding::CrLf),
            ("beta two ok", LineEnding::Lf),
            ("charlie three", LineEnding::CrLf),
            ("delta four", LineEnding::None),
        ],
    );

    assert_eq!(scanned.lines, 4);
    assert_eq!(scanned.index.newlines(), 3);
    assert_eq!(candidate_lines(&scanned), [0, 1, 2, 3]);
    assert_eq!(candidate_hashes(&scanned), hashes(&expected.keys(&options)));
}

#[test]
fn a_missing_final_newline_does_not_add_a_line() {
    let bytes = b"alpha one\nbeta two ok";
    let options = Options::default();
    let scanned = scan_bytes(bytes, TextLayout::default(), &options, 1);
    let expected = [
        normalise("alpha one", LineEnding::Lf, false, &options),
        normalise("beta two ok", LineEnding::None, false, &options),
    ];

    assert_eq!(scanned.lines, 2);
    assert_eq!(scanned.index.newlines(), 1);
    assert_eq!(candidate_hashes(&scanned), hashes(&expected));
}

#[test]
fn every_normalise_option_changes_keys_as_e7_does() {
    let (bytes, expected) = normalise_source();
    let defaults = expected.keys(&DEFAULT);
    for (name, options) in normalise_cases() {
        let scanned = scan_bytes(&bytes, expected.layout, &options, 1);
        let keys = expected.keys(&options);
        assert_eq!(candidate_hashes(&scanned), hashes(&keys), "{name}");
        if options != DEFAULT {
            assert_ne!(keys, defaults, "{name} changed no key");
        }
    }
}

#[test]
fn short_and_blank_lines_are_never_candidates() {
    let bytes = b"\n   \nshort\n1234567\n12345678\n";
    let options = Options::default();
    let scanned = scan_bytes(bytes, TextLayout::default(), &options, 1);
    let expected = [normalise("12345678", LineEnding::Lf, false, &options)];
    assert_eq!(scanned.lines, 5);
    assert_eq!(candidate_lines(&scanned), [4]);
    assert_eq!(candidate_hashes(&scanned), hashes(&expected));
}

#[test]
fn selected_and_rejected_hashes_are_both_present() {
    let lines = [
        "log line number 0 with enough text",
        "log line number 136 with enough text",
        "log line number 1 with enough text",
        "log line number 192 with enough text",
        "log line number 2 with enough text",
    ];
    let options = Options::default();
    let mut text = lines.join("\n");
    text.push('\n');
    let keys: Vec<u64> = lines
        .iter()
        .map(|line| xxh3_64(normalise(line, LineEnding::Lf, false, &options).as_bytes()))
        .collect();
    let buckets = u64::from(BUCKETS);
    let selected: Vec<u64> = keys
        .iter()
        .copied()
        .filter(|hash| hash.is_multiple_of(buckets))
        .collect();
    let rejected = keys
        .iter()
        .filter(|hash| !hash.is_multiple_of(buckets))
        .count();
    assert!(!selected.is_empty());
    assert!(rejected > 0);

    let scanned = scan_bytes(text.as_bytes(), TextLayout::default(), &options, BUCKETS);
    assert_eq!(candidate_lines(&scanned), [1, 3]);
    assert_eq!(candidate_hashes(&scanned), selected);
}

struct CountingSource {
    bytes: Vec<u8>,
    next: Cell<u64>,
    read: Cell<u64>,
    largest: Cell<usize>,
}

impl CountingSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            next: Cell::new(0),
            read: Cell::new(0),
            largest: Cell::new(0),
        }
    }
}

impl ByteSource for CountingSource {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        assert_eq!(offset, self.next.get(), "a byte range was read twice");
        let start = usize::try_from(offset).map_err(|_| "offset")?;
        let count = self.bytes.len().saturating_sub(start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        self.next.set(offset + u64::try_from(count).unwrap());
        self.read
            .set(self.read.get() + u64::try_from(count).unwrap());
        self.largest.set(self.largest.get().max(count));
        Ok(count)
    }
}

#[test]
fn a_counting_source_reads_each_byte_once() {
    let bytes = vec![b'x'; BLOCK * 2 + 123];
    let length = bytes.len();
    let source = CountingSource::new(bytes);
    let options = Options::default();
    let scanned = scan(
        &source,
        TextLayout::default(),
        &options,
        1,
        &CancelToken::new(),
        &mut |_| {},
    )
    .unwrap();

    assert_eq!(scanned.lines, 1);
    assert_eq!(scanned.index.scanned(), u64::try_from(length).unwrap());
    assert_eq!(source.read.get(), u64::try_from(length).unwrap());
    assert_eq!(source.next.get(), u64::try_from(length).unwrap());
    assert!(source.largest.get() <= BLOCK);
}

struct CancellingSource {
    bytes: Vec<u8>,
    cancel: CancelToken,
    reads: AtomicUsize,
}

impl CancellingSource {
    fn new(bytes: Vec<u8>, cancel: CancelToken) -> Self {
        Self {
            bytes,
            cancel,
            reads: AtomicUsize::new(0),
        }
    }
}

impl ByteSource for CancellingSource {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let start = usize::try_from(offset).map_err(|_| "offset")?;
        let count = self.bytes.len().saturating_sub(start).min(buf.len());
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            self.cancel.cancel();
        }
        Ok(count)
    }
}

#[test]
fn cancel_stops_within_one_block() {
    let cancel = CancelToken::new();
    let source = CancellingSource::new(vec![b'x'; BLOCK + 10], cancel.clone());
    let options = Options::default();
    let failure = scan(
        &source,
        TextLayout::default(),
        &options,
        1,
        &cancel,
        &mut |_| {},
    )
    .unwrap_err();

    assert_eq!(failure, Reason::Stopped);
    assert_eq!(source.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn cancel_before_start_reads_nothing() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let source = CountingSource::new(vec![b'x'; BLOCK + 10]);
    let options = Options::default();
    let failure = scan(
        &source,
        TextLayout::default(),
        &options,
        1,
        &cancel,
        &mut |_| {},
    )
    .unwrap_err();

    assert_eq!(failure, Reason::Stopped);
    assert_eq!(source.read.get(), 0);
}

#[test]
fn cancel_from_the_final_progress_is_reported() {
    let cancel = CancelToken::new();
    let watcher = cancel.clone();
    let options = Options::default();
    let failure = scan(
        b"alpha line one\n".as_slice(),
        TextLayout::default(),
        &options,
        1,
        &cancel,
        &mut |_| watcher.cancel(),
    )
    .unwrap_err();

    assert_eq!(failure, Reason::Stopped);
}

#[test]
fn progress_arrives_every_eight_mebibytes_and_at_the_end() {
    let length = usize::try_from(PROGRESS_EVERY).unwrap() * 2 + 5;
    let bytes = vec![b'x'; length];
    let mut ticks = Vec::new();
    let options = Options::default();
    let scanned = scan(
        bytes.as_slice(),
        TextLayout::default(),
        &options,
        1,
        &CancelToken::new(),
        &mut |done| ticks.push(done),
    )
    .unwrap();

    assert_eq!(scanned.lines, 1);
    assert_eq!(ticks, [PROGRESS_EVERY, PROGRESS_EVERY * 2, length as u64]);
}

struct ResizingSource {
    bytes: Vec<u8>,
    len: Cell<u64>,
    probe: Cell<Option<u64>>,
    changed: Cell<bool>,
}

impl ResizingSource {
    fn new(bytes: Vec<u8>, len: u64, probe: u64) -> Self {
        Self {
            bytes,
            len: Cell::new(len),
            probe: Cell::new(Some(probe)),
            changed: Cell::new(false),
        }
    }
}

impl ByteSource for ResizingSource {
    fn len(&self) -> u64 {
        self.len.get()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let start = usize::try_from(offset).map_err(|_| "offset")?;
        let end = usize::try_from(self.len.get())
            .map_err(|_| "length")?
            .min(self.bytes.len());
        let count = end.saturating_sub(start).min(buf.len());
        if count > 0 {
            buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        }
        Ok(count)
    }

    fn probe_end(&self) -> Result<(), String> {
        if let Some(length) = self.probe.take() {
            self.len.set(length);
            self.changed.set(true);
        }
        Ok(())
    }

    fn take_changed(&self) -> bool {
        self.changed.replace(false)
    }
}

#[test]
fn a_file_that_changes_size_or_stamp_is_reported() {
    for (listed, final_len) in [(16, 32), (32, 16), (32, 32)] {
        let source = ResizingSource::new(vec![b'x'; 32], listed, final_len);
        let options = Options::default();
        let failure = scan(
            &source,
            TextLayout::default(),
            &options,
            1,
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(failure, Reason::ChangedOnDisk, "{listed} -> {final_len}");
    }
}

#[test]
fn a_sixty_four_mebibyte_line_keeps_bounded_buffers() {
    let options = Options::default();
    let mut scanner = Scanner::new(TextLayout::default(), &options, 1);
    let chunk = vec![b'x'; BLOCK];
    for _ in 0..64 {
        scanner.feed(&chunk, false).unwrap();
        assert!(BLOCK + scanner.retained_capacity() < 2 * BLOCK);
    }
    let scanned = scanner.finish().unwrap();
    assert_eq!(scanned.lines, 1);
    assert_eq!(scanned.candidates.len(), 1);
}
