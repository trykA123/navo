//! Tests the install marker's literal wire format, identity checks and UTF-8 byte cap.

use crate::marker::Marker;
use crate::mode::ModeError;

/// Builds the marker represented by the fixed installer JSON fixture.
fn marker() -> Marker {
    Marker {
        pack_id: "NavoApp".into(),
        install_id: "0123456789abcdef0123456789abcdef".into(),
        created_by: "navo-setup 0.4.0".into(),
    }
}

/// Marker serialization preserves its identity and uses the required field names.
#[test]
fn mode_marker_round_trips() {
    let expected = marker();
    let rendered = expected.render().unwrap();
    assert!(rendered.contains("\"packId\""));
    assert_eq!(Marker::parse(&rendered).unwrap(), expected);
}

/// The fixed camelCase JSON wire format parses without using the renderer.
#[test]
fn mode_marker_parses_literal_camel_case_keys() {
    let text = r#"{"packId":"NavoApp","installId":"0123456789abcdef0123456789abcdef","createdBy":"navo-setup 0.4.0"}"#;
    assert_eq!(Marker::parse(text).unwrap(), marker());
}

/// Unknown, duplicate, missing, foreign and malformed marker fields fail closed.
#[test]
fn mode_marker_rejects_invalid_fields() {
    let good = marker().render().unwrap();
    for invalid in [
        good.replace("NavoApp", "OtherApp"),
        good.replace("0123456789abcdef0123456789abcdef", "not-hex"),
        good.replace(
            "0123456789abcdef0123456789abcdef",
            "g123456789abcdef0123456789abcdef",
        ),
        good.replace("navo-setup 0.4.0", ""),
        good.replace("{", "{\"unknown\":1,"),
        good.replace("{", "{\"packId\":\"NavoApp\","),
        "{}".into(),
    ] {
        assert!(Marker::parse(&invalid).is_err());
    }
}

/// Parse and render count UTF-8 bytes through 4 KiB, refusing one byte over.
#[test]
fn mode_marker_enforces_byte_limit() {
    let mut expected = marker();
    expected.created_by.clear();
    let overhead = serde_json::to_string(&expected).unwrap().len();
    for size in [4095, 4096, 4097] {
        let remaining = size - overhead;
        expected.created_by = format!("{}{}", "é".repeat(remaining / 2), " ".repeat(remaining % 2));
        let text = serde_json::to_string(&expected).unwrap();
        assert_eq!(text.len(), size);
        if size <= 4096 {
            assert_eq!(Marker::parse(&text), Ok(expected.clone()));
            assert_eq!(expected.render(), Ok(text));
        } else {
            assert_eq!(Marker::parse(&text), Err(ModeError::TooLarge));
            assert_eq!(expected.render(), Err(ModeError::TooLarge));
        }
    }
}
