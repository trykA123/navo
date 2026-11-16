use super::*;

const LONE: [u16; 3] = [0x61, 0xDC00, 0xD800];
const LONE_TEXT: &str = "a\u{100400}\u{100000}";
const INVALID: [u8; 3] = [0x61, 0x80, 0xFF];
const INVALID_TEXT: &str = "a\u{100880}\u{1008FF}";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn valid_wide_units_become_text() {
    let path = ProviderPath::from_wide(&wide(r"C:\ünï📁"));
    assert_eq!(path.as_str(), Some(r"C:\ünï📁"));
}

#[test]
fn valid_wide_units_equal_the_same_text() {
    let path = ProviderPath::from_wide(&wide(r"C:\ünï📁"));
    assert_eq!(path, ProviderPath::from_text(r"C:\ünï📁"));
}

#[test]
fn lone_surrogates_are_not_text() {
    assert_eq!(ProviderPath::from_wide(&LONE).as_str(), None);
}

#[test]
fn lone_surrogates_survive_to_wide() {
    assert_eq!(
        ProviderPath::from_wide(&LONE).to_wide(),
        Some(LONE.to_vec())
    );
}

#[test]
fn valid_bytes_become_text() {
    let path = ProviderPath::from_bytes("/home/ünï📁".as_bytes());
    assert_eq!(path.as_str(), Some("/home/ünï📁"));
}

#[test]
fn invalid_utf8_is_not_text() {
    assert_eq!(ProviderPath::from_bytes(&INVALID).as_str(), None);
}

#[test]
fn invalid_utf8_survives_to_bytes() {
    assert_eq!(
        ProviderPath::from_bytes(&INVALID).to_bytes(),
        Some(INVALID.to_vec())
    );
}

#[test]
fn text_converts_to_wide() {
    assert_eq!(ProviderPath::from_text("ü📁").to_wide(), Some(wide("ü📁")));
}

#[test]
fn text_converts_to_bytes() {
    assert_eq!(
        ProviderPath::from_text("ü📁").to_bytes(),
        Some("ü📁".as_bytes().to_vec())
    );
}

#[test]
fn wide_has_no_bytes() {
    assert_eq!(ProviderPath::from_wide(&LONE).to_bytes(), None);
}

#[test]
fn bytes_have_no_wide() {
    assert_eq!(ProviderPath::from_bytes(&INVALID).to_wide(), None);
}

#[test]
fn display_of_text_is_the_text() {
    assert_eq!(ProviderPath::from_text("ü📁").display(), "ü📁");
}

#[test]
fn display_replaces_lone_surrogates() {
    assert_eq!(
        ProviderPath::from_wide(&LONE).display(),
        "a\u{FFFD}\u{FFFD}"
    );
}

#[test]
fn display_replaces_invalid_bytes() {
    assert_eq!(
        ProviderPath::from_bytes(&INVALID).display(),
        "a\u{FFFD}\u{FFFD}"
    );
}

#[test]
fn lone_surrogates_are_escaped_in_text_form() {
    assert_eq!(ProviderPath::from_wide(&LONE).to_text(), LONE_TEXT);
}

#[test]
fn surrogate_escape_range_ends_at_dfff() {
    assert_eq!(ProviderPath::from_wide(&[0xDFFF]).to_text(), "\u{1007FF}");
}

#[test]
fn invalid_bytes_are_escaped_in_text_form() {
    assert_eq!(ProviderPath::from_bytes(&INVALID).to_text(), INVALID_TEXT);
}

#[test]
fn surrogate_escapes_decode_to_wide() {
    assert_eq!(
        ProviderPath::from_text(LONE_TEXT),
        ProviderPath::from_wide(&LONE)
    );
}

#[test]
fn byte_escapes_decode_to_bytes() {
    assert_eq!(
        ProviderPath::from_text(INVALID_TEXT),
        ProviderPath::from_bytes(&INVALID)
    );
}

#[test]
fn text_without_escapes_keeps_its_text_form() {
    assert_eq!(ProviderPath::from_text(r"C:\ünï📁").to_text(), r"C:\ünï📁");
}

#[test]
fn mixed_escapes_stay_literal_text() {
    let mixed = "a\u{100000}\u{100880}";
    assert_eq!(ProviderPath::from_text(mixed).as_str(), Some(mixed));
}

#[test]
fn empty_paths_of_every_kind_are_empty() {
    let empty = [
        ProviderPath::from_text(""),
        ProviderPath::from_wide(&[]),
        ProviderPath::from_bytes(&[]),
    ];
    assert!(empty.iter().all(ProviderPath::is_empty));
}

#[test]
fn non_empty_paths_are_not_empty() {
    let paths = [
        ProviderPath::from_text("a"),
        ProviderPath::from_wide(&LONE),
        ProviderPath::from_bytes(&INVALID),
    ];
    assert!(!paths.iter().any(ProviderPath::is_empty));
}
