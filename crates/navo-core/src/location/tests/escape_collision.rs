use proptest::prelude::*;

use super::*;

fn round_trip(path: &ProviderPath) -> ProviderPath {
    ProviderPath::from_text(&path.to_text())
}

fn text(s: &str) -> ProviderPath {
    ProviderPath::from_bytes(s.as_bytes())
}

#[test]
fn a_literal_wide_escape_character_survives() {
    let name = text("\u{100000}");
    assert_eq!(round_trip(&name), name);
}

#[test]
fn a_literal_byte_escape_character_survives() {
    let name = text("a\u{1008FF}b");
    assert_eq!(round_trip(&name), name);
}

#[test]
fn a_literal_gap_character_survives() {
    let name = text("\u{100800}\u{100801}");
    assert_eq!(round_trip(&name), name);
}

#[test]
fn a_literal_escape_character_next_to_a_lone_surrogate_survives() {
    let mut units: Vec<u16> = "\u{100000}".encode_utf16().collect();
    units.push(0xD800);
    let name = ProviderPath::from_wide(&units);
    assert_eq!(round_trip(&name), name);
}

#[test]
fn a_literal_escape_character_next_to_an_invalid_byte_survives() {
    let mut bytes = "\u{100880}".as_bytes().to_vec();
    bytes.push(0xFF);
    let name = ProviderPath::from_bytes(&bytes);
    assert_eq!(round_trip(&name), name);
}

#[test]
fn ordinary_text_keeps_its_text_form() {
    assert_eq!(text("C:\\Users\\admin").to_text(), "C:\\Users\\admin");
}

fn plane16_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('a'),
        Just('/'),
        (0x10_0000_u32..=0x10_08FF).prop_map(|c| char::from_u32(c).unwrap()),
    ]
}

proptest! {
    #[test]
    fn text_with_plane16_characters_survives(chars in prop::collection::vec(plane16_char(), 1..16)) {
        let name = text(&chars.into_iter().collect::<String>());
        prop_assert_eq!(round_trip(&name), name);
    }

    #[test]
    fn wide_units_with_plane16_pairs_survive(
        parts in prop::collection::vec(prop_oneof![
            plane16_char().prop_map(|c| c.encode_utf16(&mut [0; 2]).to_vec()),
            (0xD800_u16..=0xDFFF).prop_map(|u| vec![u]),
        ], 1..12)
    ) {
        let units: Vec<u16> = parts.concat();
        let name = ProviderPath::from_wide(&units);
        prop_assert_eq!(round_trip(&name).to_wide(), Some(units));
    }

    #[test]
    fn bytes_with_plane16_characters_survive(
        parts in prop::collection::vec(prop_oneof![
            plane16_char().prop_map(|c| c.to_string().into_bytes()),
            (0x80_u8..=0xFF).prop_map(|b| vec![b]),
        ], 1..12)
    ) {
        let bytes: Vec<u8> = parts.concat();
        let name = ProviderPath::from_bytes(&bytes);
        prop_assert_eq!(round_trip(&name).to_bytes(), Some(bytes));
    }
}
