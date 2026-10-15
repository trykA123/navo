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

fn small_numbers() -> Vec<u8> {
    (0..400_u16)
        .flat_map(|at| (at % 24 + 1).to_le_bytes())
        .collect()
}

#[test]
fn a_utf32_le_bom_is_binary() {
    let utf32: Vec<u8> = "Some notes\n"
        .chars()
        .flat_map(|character| u32::from(character).to_le_bytes())
        .collect();
    assert!(looks_binary(&with_bom(&[0xFF, 0xFE, 0, 0], &utf32)));
}

#[test]
fn a_utf16_bom_followed_by_nul_units_is_binary() {
    let mut head = with_bom(&[0xFF, 0xFE], &utf16("MZ", true));
    head.extend_from_slice(&[0; 64]);
    assert!(looks_binary(&head));
    assert!(looks_binary(
        &[&utf16("Some notes", true)[..], &[0; 8]].concat()
    ));
}

#[test]
fn a_table_of_small_numbers_is_binary_with_or_without_a_bom() {
    assert!(looks_binary(&small_numbers()));
    assert!(looks_binary(&with_bom(&[0xFF, 0xFE], &small_numbers())));
}
