use super::{utf16, with_bom};
use crate::encoding::looks_binary;

#[test]
fn png_and_elf_headers_are_binary() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x01\0\0\0\x01\0\x08\x06\0\0\0";
    let elf = b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0\x02\0\x3e\0\x01\0\0\0";
    assert!(looks_binary(png));
    assert!(looks_binary(elf));
}

#[test]
fn utf16_text_with_a_bom_is_not_binary() {
    let text = "Some notes, written on Windows.\r\n";
    assert!(!looks_binary(&with_bom(&[0xFF, 0xFE], &utf16(text, true))));
    assert!(!looks_binary(&with_bom(&[0xFE, 0xFF], &utf16(text, false))));
}

#[test]
fn utf16_text_without_a_bom_is_not_binary() {
    let text = "Some notes, written on Windows.\r\n";
    assert!(!looks_binary(&utf16(text, true)));
    assert!(!looks_binary(&utf16(text, false)));
}

#[test]
fn plain_text_is_not_binary() {
    assert!(!looks_binary(b"fn main() {}\n"));
    assert!(!looks_binary("café\n".as_bytes()));
}
