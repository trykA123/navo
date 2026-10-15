use std::fs;

use rusty_ui::window_view::{Save, SaveError};

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
    };
    let saved = write(&at(&file), &save);
    assert!(
        matches!(&saved, Err(SaveError::Failed(message)) if message.contains('日') && message.contains("line 2")),
        "{saved:?}"
    );
    assert_eq!(fs::read(&file).unwrap(), bytes);
}
