use super::tests::{round_trip_request, round_trip_response};
use super::*;

fn item(title: String, level: u8, target: LinkTarget) -> OutlineItem {
    OutlineItem {
        title,
        level,
        target,
    }
}

fn payload(title: &str, level: u8, target: LinkTarget) -> Vec<u8> {
    encode_response(&Response::Outline(
        Outline::new(vec![item(title.into(), level, target)]).unwrap(),
    ))
    .unwrap()
}

#[test]
fn outlines_round_trip_with_every_target_in_document_order() {
    assert_eq!(round_trip_request(&Request::Outline), Request::Outline);
    let items = vec![
        item(
            "Chapter é".into(),
            0,
            LinkTarget::Page {
                page: 3,
                top: Some(42.5),
            },
        ),
        item(
            "Child".into(),
            1,
            LinkTarget::Page {
                page: u32::MAX,
                top: None,
            },
        ),
        item(
            "Web".into(),
            MAX_OUTLINE_LEVEL,
            LinkTarget::Uri("https://example.org/é".into()),
        ),
        item(String::new(), 0, LinkTarget::Unsupported),
    ];
    let outline = Outline::new(items.clone()).unwrap();
    assert_eq!(outline.items(), items);
    let response = Response::Outline(outline);
    assert_eq!(round_trip_response(&response), response);
    let empty = Response::Outline(Outline::new(vec![]).unwrap());
    assert_eq!(round_trip_response(&empty), empty);
}

#[test]
fn outline_item_limits_are_validated_before_reading_items() {
    let items = vec![item(String::new(), 0, LinkTarget::Unsupported); MAX_OUTLINE_ITEMS];
    let response = Response::Outline(Outline::new(items.clone()).unwrap());
    assert_eq!(round_trip_response(&response), response);
    let mut oversized = items;
    oversized.push(item(String::new(), 0, LinkTarget::Unsupported));
    assert!(matches!(
        Outline::new(oversized),
        Err(CodecError::TooLarge { .. })
    ));
    let mut bytes = encode_response(&Response::Outline(Outline::new(vec![]).unwrap())).unwrap();
    bytes[1..5].copy_from_slice(&(MAX_OUTLINE_ITEMS as u32 + 1).to_le_bytes());
    assert!(matches!(
        decode_response(&bytes),
        Err(CodecError::TooLarge { .. })
    ));
}

#[test]
fn outline_title_limits_count_characters_instead_of_bytes() {
    let title = "🦀".repeat(MAX_TITLE_CHARS);
    let response = Response::Outline(
        Outline::new(vec![item(title.clone(), 0, LinkTarget::Unsupported)]).unwrap(),
    );
    assert_eq!(round_trip_response(&response), response);
    assert!(matches!(
        Outline::new(vec![item(format!("{title}é"), 0, LinkTarget::Unsupported)]),
        Err(CodecError::TooLarge { .. })
    ));
    let mut bytes = payload("", 0, LinkTarget::Unsupported);
    let oversized = "é".repeat(MAX_TITLE_CHARS + 1);
    bytes[5..9].copy_from_slice(&(oversized.len() as u32).to_le_bytes());
    bytes.splice(9..9, oversized.bytes());
    assert!(matches!(
        decode_response(&bytes),
        Err(CodecError::TooLarge { .. })
    ));
    bytes.truncate(9);
    bytes[5..9].copy_from_slice(&(MAX_TITLE_CHARS as u32 * 4 + 1).to_le_bytes());
    assert!(matches!(
        decode_response(&bytes),
        Err(CodecError::TooLarge { .. })
    ));
}

#[test]
fn outline_levels_over_the_limit_are_malformed() {
    let mut bytes = payload("", 0, LinkTarget::Unsupported);
    for level in [MAX_OUTLINE_LEVEL + 1, u8::MAX] {
        assert!(matches!(
            Outline::new(vec![item(String::new(), level, LinkTarget::Unsupported)]),
            Err(CodecError::Malformed(_))
        ));
        bytes[9] = level;
        assert!(matches!(
            decode_response(&bytes),
            Err(CodecError::Malformed(_))
        ));
    }
}

#[test]
fn outline_destinations_and_uris_reuse_link_validation() {
    for top in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            Outline::new(vec![item(
                String::new(),
                0,
                LinkTarget::Page {
                    page: 0,
                    top: Some(top)
                }
            )]),
            Err(CodecError::Malformed(_))
        ));
        let mut bytes = payload(
            "",
            0,
            LinkTarget::Page {
                page: 0,
                top: Some(0.0),
            },
        );
        bytes[16..20].copy_from_slice(&top.to_le_bytes());
        assert!(matches!(
            decode_response(&bytes),
            Err(CodecError::Malformed(_))
        ));
    }
    let uri = "é".repeat(MAX_URI_BYTES / 2 + 1);
    assert!(matches!(
        Outline::new(vec![item(String::new(), 0, LinkTarget::Uri(uri))]),
        Err(CodecError::TooLarge { .. })
    ));
    let mut bytes = payload("", 0, LinkTarget::Uri(String::new()));
    bytes[11..15].copy_from_slice(&(MAX_URI_BYTES as u32 + 1).to_le_bytes());
    assert!(matches!(
        decode_response(&bytes),
        Err(CodecError::TooLarge { .. })
    ));
}

#[test]
fn malformed_outline_fields_and_every_truncation_are_rejected() {
    let bytes = payload(
        "é",
        0,
        LinkTarget::Page {
            page: 3,
            top: Some(42.5),
        },
    );
    for cut in 0..bytes.len() {
        assert!(decode_response(&bytes[..cut]).is_err());
    }
    let mut invalid = bytes.clone();
    invalid.push(0);
    assert!(matches!(
        decode_response(&invalid),
        Err(CodecError::Malformed(_))
    ));
    invalid = bytes.clone();
    invalid[9] = 0xff;
    assert!(matches!(
        decode_response(&invalid),
        Err(CodecError::Malformed(_))
    ));
    invalid = payload("", 0, LinkTarget::Unsupported);
    invalid[10] = 99;
    assert!(matches!(
        decode_response(&invalid),
        Err(CodecError::Malformed(_))
    ));
    invalid = bytes;
    invalid[17] = 99;
    assert!(matches!(
        decode_response(&invalid),
        Err(CodecError::Malformed(_))
    ));
}
