use std::fs;
use std::sync::atomic::AtomicBool;

use rusty_core::paging::TextLayout;

use super::*;

fn lines(count: usize, line: &[u8]) -> Vec<u8> {
    line.repeat(count)
}

type Indexed = Result<Option<LineIndex>, String>;

fn run(path: &Path, cancel: bool) -> (Indexed, Vec<(u64, bool)>) {
    let cancel = AtomicBool::new(cancel);
    let mut calls = Vec::new();
    let mut progress = |index: &LineIndex| calls.push((index.scanned(), index.is_done()));
    let result = index_lines(path, TextLayout::default(), &cancel, &mut progress);
    (result, calls)
}

#[test]
fn indexing_counts_every_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.txt");
    fs::write(&path, lines(5000, b"hello world\n")).unwrap();
    let (result, _) = run(&path, false);
    let index = result.unwrap().unwrap();
    assert!(index.is_done());
    assert_eq!(index.newlines(), 5000);
}

#[test]
fn indexing_a_file_without_a_final_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.txt");
    let mut bytes = lines(70_000, b"abc\n");
    bytes.extend_from_slice(b"tail");
    fs::write(&path, bytes).unwrap();
    let (result, _) = run(&path, false);
    let index = result.unwrap().unwrap();
    assert!(index.is_done());
    assert_eq!(index.newlines(), 70_000);
    assert_eq!(index.scanned(), 70_000 * 4 + 4);
}

#[test]
fn indexing_reports_progress_while_it_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.txt");
    let count = 2 * 1024 * 1024;
    fs::write(&path, lines(count, b"abcdefghi\n")).unwrap();
    let len = fs::metadata(&path).unwrap().len();
    let (result, calls) = run(&path, false);
    let index = result.unwrap().unwrap();
    assert_eq!(index.newlines(), u64::try_from(count).unwrap());
    assert!(calls.len() >= 3, "only {} progress calls", calls.len());
    assert!(calls.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(calls.last().map(|call| call.0), Some(len));
}

#[test]
fn indexing_cancelled_before_it_starts_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.txt");
    fs::write(&path, lines(100, b"line\n")).unwrap();
    let (result, _) = run(&path, true);
    assert!(result.unwrap().is_none());
}

#[test]
fn indexing_a_missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (result, _) = run(&dir.path().join("nope.txt"), false);
    assert!(result.is_err());
}

struct Chopped<'a> {
    rest: &'a [u8],
    sizes: Vec<usize>,
    turn: usize,
}

impl std::io::Read for Chopped<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let size = self.sizes[self.turn % self.sizes.len()];
        self.turn += 1;
        let take = size.min(buf.len()).min(self.rest.len());
        buf[..take].copy_from_slice(&self.rest[..take]);
        self.rest = &self.rest[take..];
        Ok(take)
    }
}

fn wide_lines(count: usize, little: bool) -> Vec<u8> {
    let text = "ab\r\ncd\n".repeat(count);
    text.encode_utf16()
        .flat_map(|unit| {
            if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            }
        })
        .collect()
}

#[test]
fn short_and_odd_reads_give_the_same_utf16_line_index() {
    use rusty_core::encoding::{DetectedBy, TextEncoding};

    use super::index::index_reader;

    for (encoding, little) in [
        (encoding_rs::UTF_16LE, true),
        (encoding_rs::UTF_16BE, false),
    ] {
        let layout = TextLayout::of(TextEncoding {
            encoding,
            bom: false,
            by: DetectedBy::Chosen,
        });
        let mut bytes = wide_lines(2000, little);
        bytes.push(0x41);
        let mut whole = LineIndex::with_layout(layout);
        whole.feed(&bytes);
        whole.finish();
        for sizes in [vec![1], vec![3, 5, 2, 7], vec![4097, 1, 4095]] {
            let reader = Chopped {
                rest: &bytes,
                sizes: sizes.clone(),
                turn: 0,
            };
            let index = index_reader(reader, layout, &AtomicBool::new(false), &mut |_| {})
                .unwrap()
                .unwrap();
            assert_eq!(index, whole, "{encoding:?} {sizes:?}");
        }
    }
}
