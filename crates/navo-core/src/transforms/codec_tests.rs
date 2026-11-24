//! Tests for the editor codecs and the invisible-character scan.

use proptest::prelude::*;

use super::*;

#[test]
fn base64_round_trip_known() {
    assert_eq!(base64_encode("héllo"), "aMOpbGxv");
    assert_eq!(base64_decode("aMOpbGxv"), Ok("héllo".to_owned()));
    assert_eq!(base64_decode("aMOpbGxv\n"), Ok("héllo".to_owned()));
    assert_eq!(base64_decode("aGk"), Ok("hi".to_owned()));
}

#[test]
fn base64_errors() {
    assert_eq!(
        base64_decode("a"),
        Err(CodecError::Invalid {
            at: 1,
            reason: "length"
        })
    );
    assert_eq!(
        base64_decode("aG!k"),
        Err(CodecError::Invalid {
            at: 2,
            reason: "character"
        })
    );
    assert_eq!(base64_decode("/w=="), Err(CodecError::NotUtf8));
}

#[test]
fn url_round_trip_known() {
    assert_eq!(url_encode("a b/é~"), "a%20b%2F%C3%A9~");
    assert_eq!(url_decode("a%20b%2F%C3%A9~"), Ok("a b/é~".to_owned()));
    assert_eq!(url_decode("a+b"), Ok("a+b".to_owned()));
}

#[test]
fn url_bad_percent() {
    assert_eq!(
        url_decode("50%"),
        Err(CodecError::Invalid {
            at: 2,
            reason: "percent"
        })
    );
}

#[test]
fn json_round_trip_known() {
    let raw = "a\"b\\\n\u{1}";
    let escaped = r#"a\"b\\\n\u0001"#;
    assert_eq!(json_escape(raw), escaped);
    assert_eq!(json_unescape(escaped), Ok(raw.to_owned()));
    assert_eq!(json_unescape("\\ud83d\\ude00"), Ok("😀".to_owned()));
    assert_eq!(
        json_unescape("\\ud83d"),
        Err(CodecError::Invalid {
            at: 0,
            reason: "escape"
        })
    );
}

#[test]
fn scan_finds_each_kind() {
    let text = "\u{feff}a\u{200b}b\u{200f}c\u{a0}d\u{201c}e\u{feff}";
    let found = scan_invisible(text);
    let kinds: Vec<InvisibleKind> = found.iter().map(|item| item.kind).collect();
    assert_eq!(
        kinds,
        vec![
            InvisibleKind::ZeroWidth,
            InvisibleKind::BidiMark,
            InvisibleKind::NoBreakSpace,
            InvisibleKind::SmartQuote,
            InvisibleKind::Bom,
        ]
    );
    assert_eq!(found[0].at, 4);
}

proptest! {
    #[test]
    fn base64_round_trips(s in any::<String>()) {
        prop_assert_eq!(base64_decode(&base64_encode(&s)), Ok(s));
    }

    #[test]
    fn url_round_trips(s in any::<String>()) {
        prop_assert_eq!(url_decode(&url_encode(&s)), Ok(s));
    }

    #[test]
    fn json_round_trips(s in any::<String>()) {
        prop_assert_eq!(json_unescape(&json_escape(&s)), Ok(s));
    }
}
