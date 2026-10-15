use encoding_rs::{SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1251, WINDOWS_1252};

use super::{FALLBACK, FRENCH, bytes_in, found, with_bom};
use crate::encoding::{DetectedBy, detect};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

#[test]
fn every_bom_names_its_encoding() {
    let cases = [
        (with_bom(UTF8_BOM, b"plain"), UTF_8),
        (with_bom(&[0xFF, 0xFE], &[b'a', 0]), UTF_16LE),
        (with_bom(&[0xFE, 0xFF], &[0, b'a']), UTF_16BE),
    ];
    for (bytes, encoding) in cases {
        assert_eq!(
            detect(&bytes, FALLBACK),
            found(encoding, true, DetectedBy::Bom),
            "{}",
            encoding.name()
        );
    }
}

#[test]
fn an_xml_declaration_names_utf8() {
    let bytes = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<menu>café</menu>\n";
    assert_eq!(
        detect(bytes.as_bytes(), FALLBACK),
        found(UTF_8, false, DetectedBy::Declaration)
    );
}

#[test]
fn an_xml_declaration_names_windows_1252() {
    let text = "<?xml version='1.0' encoding='windows-1252'?>\n<menu>café crème</menu>\n";
    assert_eq!(
        detect(&bytes_in(WINDOWS_1252, text), FALLBACK),
        found(WINDOWS_1252, false, DetectedBy::Declaration)
    );
}

#[test]
fn an_html_meta_charset_names_the_encoding() {
    let text = "<!doctype html>\n<html><head><meta charset=\"windows-1251\"><title>Привет</title></head></html>\n";
    assert_eq!(
        detect(&bytes_in(WINDOWS_1251, text), FALLBACK),
        found(WINDOWS_1251, false, DetectedBy::Declaration)
    );
}

#[test]
fn ascii_and_valid_utf8_are_utf8_without_a_bom() {
    for text in ["", "plain ascii\n", "naïve café, 日本語, 😀\n"] {
        assert_eq!(
            detect(text.as_bytes(), FALLBACK),
            found(UTF_8, false, DetectedBy::Utf8),
            "{text:?}"
        );
    }
}

#[test]
fn german_windows_1252_is_guessed() {
    let text = "Größere Änderungen für die Straße: Müller übernimmt die Prüfung, während Bäcker über Gebühren spricht.\n";
    assert_eq!(
        detect(&bytes_in(WINDOWS_1252, text), FALLBACK),
        found(WINDOWS_1252, false, DetectedBy::Guess)
    );
}

#[test]
fn french_windows_1252_is_guessed() {
    assert_eq!(
        detect(&bytes_in(WINDOWS_1252, FRENCH), FALLBACK),
        found(WINDOWS_1252, false, DetectedBy::Guess)
    );
}

#[test]
fn cyrillic_windows_1251_is_guessed() {
    let text = "Привет, как дела? Это проверка кодировки для русского текста, который должен быть распознан правильно.\n";
    assert_eq!(
        detect(&bytes_in(WINDOWS_1251, text), FALLBACK),
        found(WINDOWS_1251, false, DetectedBy::Guess)
    );
}

#[test]
fn shift_jis_is_guessed() {
    let text = "日本語のテキストです。これは文字コードの判定を確認するための文章です。\n";
    assert_eq!(
        detect(&bytes_in(SHIFT_JIS, text), FALLBACK),
        found(SHIFT_JIS, false, DetectedBy::Guess)
    );
}

#[test]
fn a_single_stray_byte_is_too_little_to_guess() {
    assert_eq!(
        detect(b"x\xE9\n", FALLBACK),
        found(FALLBACK, false, DetectedBy::Fallback)
    );
}

#[test]
fn a_bom_beats_a_contradicting_declaration() {
    let body = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?>\n<menu/>\n";
    assert_eq!(
        detect(&with_bom(UTF8_BOM, body), FALLBACK),
        found(UTF_8, true, DetectedBy::Bom)
    );
}

#[test]
fn an_unknown_declared_label_is_ignored() {
    let text = "<?xml version=\"1.0\" encoding=\"x-no-such-charset\"?>\n<menu>café</menu>\n";
    assert_eq!(
        detect(text.as_bytes(), FALLBACK),
        found(UTF_8, false, DetectedBy::Utf8)
    );
}
