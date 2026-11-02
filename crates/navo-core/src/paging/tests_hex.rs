use super::{HEX_WIDTH, hex_row, hex_row_start, hex_rows};

fn counting(len: u8) -> Vec<u8> {
    (0..len).collect()
}

#[test]
fn hex_rows_is_zero_for_an_empty_file() {
    assert_eq!(hex_rows(0), 0);
}

#[test]
fn hex_rows_counts_a_partial_row() {
    assert_eq!(hex_rows(1), 1);
    assert_eq!(hex_rows(15), 1);
    assert_eq!(hex_rows(17), 2);
}

#[test]
fn hex_rows_counts_full_rows_exactly() {
    assert_eq!(hex_rows(16), 1);
    assert_eq!(hex_rows(32), 2);
    assert_eq!(hex_rows(16 * 1000), 1000);
}

#[test]
fn hex_rows_handles_huge_lengths() {
    assert_eq!(hex_rows(u64::MAX), u64::MAX / HEX_WIDTH + 1);
}

#[test]
fn hex_row_start_rounds_down_to_sixteen() {
    assert_eq!(hex_row_start(0), 0);
    assert_eq!(hex_row_start(15), 0);
    assert_eq!(hex_row_start(16), 16);
    assert_eq!(hex_row_start(33), 32);
    assert_eq!(hex_row_start(0x1234_5678), 0x1234_5670);
}

#[test]
fn hex_row_offset_has_eight_uppercase_digits() {
    assert_eq!(hex_row(0, &[], false).offset, "00000000");
    assert_eq!(hex_row(0xABC0, &[], false).offset, "0000ABC0");
}

#[test]
fn hex_row_wide_offset_has_sixteen_digits() {
    assert_eq!(hex_row(0, &[], true).offset, "0000000000000000");
    assert_eq!(hex_row(0x1_0000_00F0, &[], true).offset, "00000001000000F0");
}

#[test]
fn hex_row_full_row_has_a_gap_after_eight_bytes() {
    let row = hex_row(0, &counting(16), false);
    assert_eq!(row.hex, "00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F");
}

#[test]
fn hex_row_uses_uppercase_digits() {
    let row = hex_row(0, &[0xAB, 0xCD, 0xEF], false);
    assert!(row.hex.starts_with("AB CD EF"));
}

#[test]
fn hex_row_pads_a_short_row_to_48_chars() {
    let row = hex_row(0, &[0x41, 0x42, 0x43], false);
    assert_eq!(row.hex, format!("{:<48}", "41 42 43"));
}

#[test]
fn hex_row_pads_a_short_row_past_the_gap() {
    let row = hex_row(16, &counting(10), false);
    assert_eq!(row.hex, format!("{:<48}", "00 01 02 03 04 05 06 07  08 09"));
}

#[test]
fn hex_row_hex_is_always_48_chars() {
    for len in 0..=16 {
        assert_eq!(hex_row(0, &counting(len), false).hex.len(), 48, "{len}");
    }
}

#[test]
fn hex_row_empty_bytes_give_blank_hex() {
    assert_eq!(hex_row(0, &[], false).hex, " ".repeat(48));
}

#[test]
fn hex_row_ascii_shows_printable_bytes() {
    let row = hex_row(0, b"Hi ~!", false);
    assert_eq!(row.ascii, "Hi ~!");
}

#[test]
fn hex_row_ascii_shows_dots_for_other_bytes() {
    let row = hex_row(0, &[0x00, 0x1F, 0x20, 0x7E, 0x7F, 0x80, 0xFF, b'A'], false);
    assert_eq!(row.ascii, ".. ~...A");
}

#[test]
fn hex_row_ascii_has_one_char_per_present_byte() {
    assert_eq!(hex_row(0, b"abc", false).ascii, "abc");
    assert_eq!(hex_row(0, &[], false).ascii, "");
}

#[test]
fn hex_row_utf8_bytes_show_as_dots() {
    let row = hex_row(0, "é".as_bytes(), false);
    assert_eq!(row.ascii, "..");
}
