use super::text::{address, ascii, columns, gap_row, hex_cells, parse_address, span};

#[test]
fn addresses_show_eight_digits_unless_the_image_is_wide() {
    assert_eq!(address(0x1_0400, false), "0x00010400");
    assert_eq!(address(0x1_0400, true), "0x0000000000010400");
}

#[test]
fn a_span_names_its_last_byte() {
    assert_eq!(span(&(0x100..0x140), false), "0x00000100–0x0000013F");
}

#[test]
fn hex_cells_split_the_row_in_two_halves_and_mark_missing_bytes() {
    let mut bytes = [Some(0xAB); 16];
    bytes[1] = None;
    let (text, missing) = hex_cells(&bytes);
    assert_eq!(text, "AB -- AB AB AB AB AB AB  AB AB AB AB AB AB AB AB");
    assert_eq!(missing, vec![3..5]);
}

#[test]
fn ascii_shows_dots_for_control_bytes_and_blanks_for_missing_ones() {
    assert_eq!(ascii(&[Some(b'H'), Some(0), None, Some(b'i')]), "H. i");
}

#[test]
fn a_gap_row_says_how_much_is_not_in_the_file() {
    assert_eq!(
        gap_row(&(0x0..0x1_0000), false),
        "gap 0x00000000–0x0000FFFF · 64.0 KB not in file"
    );
}

#[test]
fn addresses_parse_with_or_without_a_prefix() {
    assert_eq!(parse_address("0x8000400"), Ok(0x800_0400));
    assert_eq!(parse_address(" 1F00 "), Ok(0x1F00));
    assert!(parse_address("zz").is_err());
}

#[test]
fn the_column_header_follows_the_row_width() {
    assert_eq!(
        columns(16),
        "00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F"
    );
    let wide = columns(32);
    assert!(wide.ends_with("18 19 1A 1B 1C 1D 1E 1F"));
    assert_eq!(wide.len(), hex_cells(&[Some(0); 32]).0.len());
}
