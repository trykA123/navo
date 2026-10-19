use std::fs;

use rusty_ui::window_view::{FileText, Save, SaveError};

use super::at;
use crate::files::{read, write};

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

#[test]
fn a_utf16_file_reads_edits_and_saves_back_as_utf16() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("wide.txt");
    fs::write(&file, utf16_le_with_bom("one\ntwo\n")).unwrap();
    let opened = read(&at(&file));
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    assert_eq!(opened.text, "one\ntwo\n");
    assert!(!opened.read_only);
    assert_eq!(opened.encoding.label(), "UTF-16 LE BOM");
    let edited = format!("{}three\n", opened.text);
    let save = Save {
        text: &edited,
        crlf: opened.crlf,
        expected: opened.stamp,
        encoding: opened.encoding,
        ends: None,
    };
    write(&at(&file), &save).unwrap();
    assert_eq!(
        fs::read(&file).unwrap(),
        utf16_le_with_bom("one\ntwo\nthree\n")
    );
}

#[test]
fn a_windows_1252_file_shows_accents_and_saves_back_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("menu.txt");
    let bytes = latin1(FRENCH);
    fs::write(&file, &bytes).unwrap();
    let opened = read(&at(&file)).unwrap();
    assert_eq!(opened.text, FRENCH);
    assert!(!opened.read_only);
    assert_eq!(opened.encoding.label(), "Windows-1252");
    let save = Save {
        text: &opened.text,
        crlf: opened.crlf,
        expected: opened.stamp,
        encoding: opened.encoding,
        ends: None,
    };
    write(&at(&file), &save).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
}

#[test]
fn a_character_the_encoding_cannot_hold_fails_the_save_and_keeps_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("menu.txt");
    let bytes = latin1(FRENCH);
    fs::write(&file, &bytes).unwrap();
    let opened = read(&at(&file)).unwrap();
    let edited = format!("{}日\n", opened.text);
    let save = Save {
        text: &edited,
        crlf: opened.crlf,
        expected: opened.stamp,
        encoding: opened.encoding,
        ends: None,
    };
    let saved = write(&at(&file), &save);
    assert!(
        matches!(&saved, Err(SaveError::Failed(message)) if message.contains('日') && message.contains("line 2")),
        "{saved:?}"
    );
    assert_eq!(fs::read(&file).unwrap(), bytes);
}

const GERMAN: &str = "Größere Änderungen für die Straße: Müller übernimmt die Prüfung, während Bäcker über Gebühren spricht.\n";
const JAPANESE: &str = "日本語のテキストです。これは文字コードの判定を確認するための文章です。\n";

fn opened(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf, FileText) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(name);
    fs::write(&file, bytes).unwrap();
    let opened = read(&at(&file));
    assert!(opened.is_ok(), "{opened:?}");
    (dir, file, opened.unwrap())
}

#[test]
fn utf8_cut_off_in_its_last_character_opens_read_only_as_utf8() {
    let (_dir, _, opened) = opened("cut.txt", &[GERMAN.as_bytes(), &[0xC3]].concat());
    assert!(opened.text.starts_with(GERMAN), "{:?}", opened.text);
    assert_eq!(opened.encoding.label(), "UTF-8");
    assert!(opened.read_only);
}

#[test]
fn utf8_with_one_stray_byte_opens_read_only_as_utf8() {
    let (start, end) = JAPANESE.split_at(JAPANESE.len() / 2 / 3 * 3);
    let (_dir, _, opened) = opened(
        "stray.txt",
        &[start.as_bytes(), &[0xFF], end.as_bytes()].concat(),
    );
    assert_eq!(opened.text, format!("{start}\u{FFFD}{end}"));
    assert_eq!(opened.encoding.label(), "UTF-8");
    assert!(opened.read_only);
}

#[test]
fn a_utf16_file_with_crlf_saved_unedited_is_byte_identical() {
    let bytes = utf16_le_with_bom("a\r\nb\r\n");
    let (_dir, file, opened) = opened("crlf.txt", &bytes);
    assert!(!opened.read_only);
    let save = Save {
        text: &opened.text,
        crlf: opened.crlf,
        expected: opened.stamp,
        encoding: opened.encoding,
        ends: None,
    };
    write(&at(&file), &save).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
}

#[test]
fn a_utf16_file_without_a_bom_reads_as_utf16() {
    let bytes: Vec<u8> = "Some notes, written on Windows.\r\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let (_dir, _, opened) = opened("wide.txt", &bytes);
    assert_eq!(opened.text, "Some notes, written on Windows.\r\n");
    assert_eq!(opened.encoding.label(), "UTF-16 LE");
    assert!(!opened.read_only);
}

#[test]
fn a_utf32_file_is_refused_as_binary() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("wide32.txt");
    let utf32 = "Some notes\n"
        .chars()
        .flat_map(|character| u32::from(character).to_le_bytes());
    fs::write(
        &file,
        [0xFF, 0xFE, 0, 0]
            .into_iter()
            .chain(utf32)
            .collect::<Vec<u8>>(),
    )
    .unwrap();
    assert!(read(&at(&file)).unwrap_err().contains("binary"));
}

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn read_only_or_binary(name: &str, bytes: &[u8]) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(name);
    fs::write(&file, bytes).unwrap();
    match read(&at(&file)) {
        Ok(opened) => assert!(opened.read_only, "{name} opened editable"),
        Err(message) => assert!(message.contains("binary"), "{message}"),
    }
}

#[test]
fn a_script_with_a_binary_payload_is_not_editable() {
    let header = "#!/bin/sh\necho installing\n".repeat(400);
    assert!(header.len() > 8 * 1024);
    let mut payload = noise(200 * 1024);
    payload[1000] = 0;
    payload[1001..1003].copy_from_slice(b"\r\n");
    let bytes = [header.as_bytes(), &payload].concat();
    read_only_or_binary("setup.sh", &bytes);
}

#[test]
fn bomless_utf16_followed_by_zero_bytes_is_not_editable() {
    let text = "Some notes, written on Windows.\r\n".repeat(64);
    let wide: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let bytes = [&wide[..4096], &[0; 4096]].concat();
    read_only_or_binary("wide.txt", &bytes);
}

#[test]
fn short_near_utf8_opens_read_only_as_utf8() {
    let (_dir, _, opened) = opened(
        "koeln.txt",
        &["Grüße aus Köln\n".as_bytes(), &[0xFF]].concat(),
    );
    assert_eq!(opened.encoding.label(), "UTF-8");
    assert!(opened.read_only);
}
