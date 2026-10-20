use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use encoding_rs::{UTF_8, UTF_16LE, WINDOWS_1252};
use rusty_core::paging::{ByteSource, row_at, row_start_at};
use rusty_ui::window_view::{FileText, Save};

use super::at;
use crate::files::{paged::paged, paged::paged_as, read, read_as, write};

const FRENCH: &str =
    "Le café était déjà prêt à côté de l'hôpital, où la clé du théâtre restait oubliée.\n";

fn latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|character| u8::try_from(u32::from(character)).unwrap())
        .collect()
}

fn utf16_le_with_bom(text: &str) -> Vec<u8> {
    let units = text.encode_utf16().flat_map(u16::to_le_bytes);
    [0xFF, 0xFE].into_iter().chain(units).collect()
}

fn stored(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn saved_unedited(path: &Path, opened: &FileText) {
    let save = Save {
        text: &opened.text,
        crlf: opened.crlf,
        expected: opened.stamp,
        encoding: opened.encoding,
        ends: opened.ends.as_deref(),
    };
    write(&at(path), &save).unwrap();
}

#[test]
fn reading_as_a_chosen_encoding_decodes_with_it_and_auto_detects_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = stored(&dir, "menu.txt", &latin1(FRENCH));
    let detected = read(&at(&path)).unwrap();
    assert_eq!(detected.text, FRENCH);
    assert_eq!(detected.encoding.label(), "Windows-1252");
    let as_utf8 = read_as(&at(&path), UTF_8).unwrap();
    assert!(as_utf8.text.contains('\u{FFFD}'));
    assert_eq!(as_utf8.encoding.label(), "UTF-8");
    let as_latin = read_as(&at(&path), WINDOWS_1252).unwrap();
    assert_eq!(as_latin.text, FRENCH);
    assert_eq!(read(&at(&path)).unwrap().text, FRENCH);
}

#[test]
fn a_lossy_chosen_decode_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = stored(&dir, "menu.txt", &latin1(FRENCH));
    assert!(read_as(&at(&path), UTF_8).unwrap().read_only);
    assert!(!read_as(&at(&path), WINDOWS_1252).unwrap().read_only);
}

#[test]
fn a_chosen_encoding_opens_a_file_that_auto_detection_calls_binary() {
    let dir = tempfile::tempdir().unwrap();
    let path = stored(
        &dir,
        "odd.txt",
        &[b"abc\0def\n".as_slice(), &[0; 2]].concat(),
    );
    assert!(read(&at(&path)).unwrap_err().contains("binary"));
    let chosen = read_as(&at(&path), WINDOWS_1252).unwrap();
    assert!(chosen.read_only);
}

#[test]
fn a_mixed_file_opens_editable_with_one_line_ending_style_in_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = stored(&dir, "mixed.txt", b"one\r\ntwo\nthree\r\nfour\nfive");
    let opened = read(&at(&path)).unwrap();
    assert!(!opened.read_only);
    assert_eq!(opened.text, "one\ntwo\nthree\nfour\nfive");
    assert!(opened.ends.is_some());
}

#[test]
fn editing_one_line_of_a_mixed_file_keeps_every_other_line_ending() {
    let dir = tempfile::tempdir().unwrap();
    let path = stored(&dir, "mixed.txt", b"one\r\ntwo\nthree\r\nfour\nfive");
    let opened = read(&at(&path)).unwrap();
    let edited = opened.text.replace("three", "THREE and more");
    let save = Save {
        text: &edited,
        crlf: opened.crlf,
        expected: opened.stamp,
        encoding: opened.encoding,
        ends: opened.ends.as_deref(),
    };
    write(&at(&path), &save).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"one\r\ntwo\nTHREE and more\r\nfour\nfive"
    );
}

const STRAY: &str = "a\r\r\nb\nc";

fn encoded_every_way(text: &str) -> [(&'static str, Vec<u8>); 3] {
    [
        ("utf8.txt", text.as_bytes().to_vec()),
        ("latin.txt", latin1(text)),
        ("wide.txt", utf16_le_with_bom(text)),
    ]
}

#[test]
fn an_unedited_mixed_file_saves_byte_identical_in_every_encoding() {
    let dir = tempfile::tempdir().unwrap();
    for text in ["d\u{e9}j\u{e0}\r\nthé\nsucre\r\n\r\nlait\n", "a\rb\r\nc\nd"] {
        for (name, bytes) in encoded_every_way(text) {
            let path = stored(&dir, name, &bytes);
            let opened = read(&at(&path)).unwrap();
            assert!(!opened.read_only, "{name}");
            assert!(opened.ends.is_some(), "{name}");
            saved_unedited(&path, &opened);
            assert_eq!(fs::read(&path).unwrap(), bytes, "{name}");
        }
    }
}

#[test]
fn an_unedited_file_with_a_lone_cr_or_a_doubled_cr_saves_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    for text in [STRAY, "a\rb\rc"] {
        for (name, bytes) in encoded_every_way(text) {
            let path = stored(&dir, name, &bytes);
            let opened = read(&at(&path)).unwrap();
            saved_unedited(&path, &opened);
            assert_eq!(fs::read(&path).unwrap(), bytes, "{text:?} {name}");
        }
    }
}

#[test]
fn a_mixed_file_with_a_cr_before_a_line_end_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in encoded_every_way(STRAY) {
        let path = stored(&dir, name, &bytes);
        assert!(read(&at(&path)).unwrap().read_only, "{name}");
    }
}

const LINES: usize = 520_000;

fn big_text(line: impl Fn(usize) -> String) -> String {
    (0..LINES).map(line).collect()
}

fn line_at(source: &dyn ByteSource, at: u64) -> (u64, String) {
    let layout = source.layout();
    let start = row_start_at(source, at).unwrap();
    let row = row_at(source, start).unwrap();
    let skip = if start == 0 {
        layout.bom_len() as u64
    } else {
        0
    };
    let mut bytes = vec![0u8; usize::try_from(row.end - start - skip).unwrap()];
    rusty_core::paging::read_full(source, start + skip, &mut bytes).unwrap();
    (start, layout.decode(&bytes))
}

fn lines_counted(opened: &rusty_ui::window_view::Paged) -> u64 {
    let cancel = AtomicBool::new(false);
    let index = (opened.index)(&cancel, &mut |_| {}).unwrap().unwrap();
    index.newlines()
}

#[test]
fn a_20_mb_utf16_file_opens_as_paged_text_with_decoded_rows() {
    let dir = tempfile::tempdir().unwrap();
    let text = big_text(|n| format!("line {n} wörld 日本語\r\n"));
    let bytes = utf16_le_with_bom(&text);
    assert!(bytes.len() > 20 * 1024 * 1024);
    let path = stored(&dir, "wide.log", &bytes);
    let opened = paged(&at(&path)).unwrap();
    let layout = opened.source.layout();
    assert_eq!(layout.text.label(), "UTF-16 LE BOM");
    assert_eq!(opened.problem, None);
    assert_eq!(opened.source.len(), bytes.len() as u64);
    assert_eq!(line_at(&*opened.source, 0).1, "line 0 wörld 日本語");
    let (start, middle) = line_at(&*opened.source, bytes.len() as u64 / 2 + 1);
    let number: usize = middle.split(' ').nth(1).unwrap().parse().unwrap();
    assert_eq!(middle, format!("line {number} wörld 日本語"));
    assert_eq!(start % 2, 0);
    let last = line_at(&*opened.source, bytes.len() as u64 - 1).1;
    assert_eq!(last, format!("line {} wörld 日本語", LINES - 1));
    assert_eq!(lines_counted(&opened), LINES as u64);
}

#[test]
fn a_20_mb_windows_1252_file_opens_as_paged_text_with_decoded_rows() {
    let dir = tempfile::tempdir().unwrap();
    let text = big_text(|n| format!("ligne {n} très déjà prêt à côté de l'hôpital\n"));
    let bytes = latin1(&text);
    assert!(bytes.len() > 20 * 1024 * 1024);
    let path = stored(&dir, "latin.log", &bytes);
    let opened = paged(&at(&path)).unwrap();
    assert_eq!(opened.source.layout().text.label(), "Windows-1252");
    assert_eq!(opened.problem, None);
    assert_eq!(
        line_at(&*opened.source, 0).1,
        "ligne 0 très déjà prêt à côté de l'hôpital"
    );
    let (_, middle) = line_at(&*opened.source, bytes.len() as u64 / 2);
    assert!(
        middle.ends_with("très déjà prêt à côté de l'hôpital"),
        "{middle}"
    );
    assert_eq!(lines_counted(&opened), LINES as u64);
}

#[test]
fn a_chosen_encoding_that_does_not_fit_the_head_is_reported_as_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let path = stored(&dir, "latin.log", &latin1(&FRENCH.repeat(50)));
    assert_eq!(paged(&at(&path)).unwrap().problem, None);
    let wrong = paged_as(&at(&path), UTF_8).unwrap();
    assert_eq!(wrong.source.layout().text.label(), "UTF-8");
    assert!(wrong.problem.is_some_and(|text| text.contains("UTF-8")));
    let wide = paged_as(&at(&path), UTF_16LE).unwrap();
    assert_eq!(wide.source.layout().text.label(), "UTF-16 LE");
}
