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

const GERMAN: &str = "Größere Änderungen für die Straße: Müller übernimmt die Prüfung, während Bäcker über Gebühren spricht.\n";
const JAPANESE: &str = "日本語のテキストです。これは文字コードの判定を確認するための文章です。\n";

#[test]
fn utf8_cut_off_in_its_last_character_is_still_utf8() {
    let bytes = [GERMAN.as_bytes(), &[0xC3]].concat();
    assert_eq!(
        detect(&bytes, FALLBACK),
        found(UTF_8, false, DetectedBy::Guess)
    );
}

#[test]
fn utf8_with_one_stray_byte_is_still_utf8() {
    let (start, end) = JAPANESE.split_at(JAPANESE.len() / 2 / 3 * 3);
    let bytes = [start.as_bytes(), &[0xFF], end.as_bytes()].concat();
    assert_eq!(
        detect(&bytes, FALLBACK),
        found(UTF_8, false, DetectedBy::Guess)
    );
}

#[test]
fn a_meta_tag_mentioned_outside_html_is_ignored() {
    let text = "# Notes\n\nPut <meta charset=\"koi8-r\"> in the head. Привет, мир.\n";
    assert_eq!(
        detect(text.as_bytes(), FALLBACK),
        found(UTF_8, false, DetectedBy::Utf8)
    );
}

#[test]
fn an_html_meta_after_leading_whitespace_counts() {
    let text = "\n  <HTML><head><meta charset=windows-1251></head><body>Привет</body></html>\n";
    assert_eq!(
        detect(&bytes_in(WINDOWS_1251, text), FALLBACK),
        found(WINDOWS_1251, false, DetectedBy::Declaration)
    );
}

#[test]
fn a_label_for_the_replacement_encoding_is_ignored() {
    let text = "<?xml version=\"1.0\" encoding=\"ISO-2022-KR\"?>\n<menu>café</menu>\n";
    assert_eq!(
        detect(text.as_bytes(), FALLBACK),
        found(UTF_8, false, DetectedBy::Utf8)
    );
}

#[test]
fn a_utf32_le_bom_is_not_taken_for_utf16() {
    let utf32: Vec<u8> = "Some notes\n"
        .chars()
        .flat_map(|character| u32::from(character).to_le_bytes())
        .collect();
    let detected = detect(&with_bom(&[0xFF, 0xFE, 0, 0], &utf32), FALLBACK);
    assert_ne!(detected.encoding, UTF_16LE);
    assert_ne!(detected.by, DetectedBy::Bom);
}

#[test]
fn a_table_of_small_numbers_is_not_guessed_as_utf16() {
    let table: Vec<u8> = (0..400_u16)
        .flat_map(|at| (at % 24 + 1).to_le_bytes())
        .collect();
    assert_ne!(detect(&table, FALLBACK).encoding, UTF_16LE);
}

#[test]
fn only_the_first_64_kb_feed_the_guess() {
    let ascii = "plain ascii line\n".repeat(64 * 1024 / 17 + 1);
    let bytes = [ascii.as_bytes(), &bytes_in(WINDOWS_1252, GERMAN)].concat();
    assert_eq!(
        detect(&bytes, FALLBACK),
        found(FALLBACK, false, DetectedBy::Fallback)
    );
}

#[test]
fn short_utf8_with_a_stray_or_cut_tail_is_still_utf8() {
    let cases = [
        ["Grüße aus Köln\n".as_bytes(), &[0xFF]].concat(),
        ["Notiz: Grüße\n".as_bytes(), &[0xA0]].concat(),
        [&b"abc"[..], &[0xC3]].concat(),
    ];
    for bytes in cases {
        assert_eq!(
            detect(&bytes, FALLBACK),
            found(UTF_8, false, DetectedBy::Guess),
            "{bytes:?}"
        );
    }
}

#[test]
fn short_windows_1252_is_not_taken_for_utf8() {
    let short = bytes_in(WINDOWS_1252, "café naïve");
    assert_ne!(detect(&short, FALLBACK).encoding, UTF_8);
    assert_eq!(detect(&short, WINDOWS_1252).encoding, WINDOWS_1252);
    let longer = bytes_in(
        WINDOWS_1252,
        "Un café naïve, une crème brûlée et un gâteau.",
    );
    assert_eq!(detect(&longer, FALLBACK).encoding, WINDOWS_1252);
}

#[test]
fn a_charset_outside_the_meta_tag_is_ignored() {
    let text = "<!doctype html>\n<html><head><meta name=\"description\"><title>Use charset=koi8-r here</title></head><body>Привет</body></html>\n";
    assert_eq!(
        detect(text.as_bytes(), FALLBACK),
        found(UTF_8, false, DetectedBy::Utf8)
    );
}

#[test]
fn a_charset_in_a_later_meta_tag_counts() {
    let text = "<!doctype html>\n<html><head><meta name=\"viewport\" content=\"width=device-width\"><meta charset=\"windows-1251\"><title>Привет</title></head></html>\n";
    assert_eq!(
        detect(&bytes_in(WINDOWS_1251, text), FALLBACK),
        found(WINDOWS_1251, false, DetectedBy::Declaration)
    );
}

#[test]
fn utf8_under_a_single_byte_declaration_is_utf8() {
    let text = "<?xml version='1.0' encoding='windows-1252'?>\n<menu>café crème</menu>\n";
    assert_eq!(
        detect(text.as_bytes(), FALLBACK),
        found(UTF_8, false, DetectedBy::Utf8)
    );
    let ascii = "<?xml version='1.0' encoding='windows-1252'?>\n<menu/>\n";
    assert_eq!(
        detect(ascii.as_bytes(), FALLBACK),
        found(WINDOWS_1252, false, DetectedBy::Declaration)
    );
}
