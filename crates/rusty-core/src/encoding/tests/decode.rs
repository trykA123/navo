use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::{FALLBACK, FRENCH, bytes_in, found, utf16, with_bom};
use crate::encoding::{DetectedBy, TextEncoding, decode, decode_as};

#[test]
fn the_bom_is_stripped_from_the_text() {
    let decoded = decode(&with_bom(&[0xEF, 0xBB, 0xBF], b"hello\n"), FALLBACK);
    assert_eq!(decoded.text, "hello\n");
    assert_eq!(decoded.encoding, found(UTF_8, true, DetectedBy::Bom));
}

#[test]
fn utf16_little_endian_decodes() {
    let text = "héllo 日本 😀\n";
    let decoded = decode(&with_bom(&[0xFF, 0xFE], &utf16(text, true)), FALLBACK);
    assert_eq!(decoded.text, text);
    assert_eq!(decoded.encoding, found(UTF_16LE, true, DetectedBy::Bom));
    assert!(decoded.lossless);
}

#[test]
fn utf16_big_endian_decodes() {
    let text = "héllo 日本 😀\n";
    let decoded = decode(&with_bom(&[0xFE, 0xFF], &utf16(text, false)), FALLBACK);
    assert_eq!(decoded.text, text);
    assert_eq!(decoded.encoding, found(UTF_16BE, true, DetectedBy::Bom));
    assert!(decoded.lossless);
}

#[test]
fn a_clean_windows_1252_file_decodes_losslessly() {
    let decoded = decode(&bytes_in(WINDOWS_1252, FRENCH), FALLBACK);
    assert_eq!(decoded.text, FRENCH);
    assert!(decoded.lossless);
}

#[test]
fn invalid_utf8_forced_as_utf8_is_not_lossless() {
    let decoded = decode_as(b"\xE9t\xE9 \xFF\n", UTF_8);
    assert!(decoded.text.contains('\u{FFFD}'));
    assert!(!decoded.lossless);
}

#[test]
fn choosing_windows_1252_for_utf8_gives_mojibake() {
    let decoded = decode_as("été\n".as_bytes(), WINDOWS_1252);
    assert_eq!(decoded.text, "Ã©tÃ©\n");
    assert_eq!(
        decoded.encoding,
        found(WINDOWS_1252, false, DetectedBy::Chosen)
    );
}

#[test]
fn a_chosen_encoding_keeps_and_strips_its_matching_bom() {
    let decoded = decode_as(&with_bom(&[0xFF, 0xFE], &utf16("hi", true)), UTF_16LE);
    assert_eq!(decoded.text, "hi");
    assert_eq!(
        decoded.encoding,
        TextEncoding {
            encoding: UTF_16LE,
            bom: true,
            by: DetectedBy::Chosen
        }
    );
}

#[test]
fn text_with_a_nul_is_not_lossless() {
    let decoded = decode(b"#!/bin/sh\necho hi\n\0\x01tail\n", FALLBACK);
    assert!(decoded.text.contains('\0'));
    assert!(!decoded.lossless);
}
