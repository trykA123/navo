use std::io::Cursor;

use super::*;

fn round_trip_request(request: &Request) -> Request {
    let mut wire = Vec::new();
    write_request(&mut wire, request).unwrap();
    read_request(&mut Cursor::new(wire)).unwrap().unwrap()
}

fn round_trip_response(response: &Response) -> Response {
    let mut wire = Vec::new();
    write_response(&mut wire, response).unwrap();
    read_response(&mut Cursor::new(wire)).unwrap().unwrap()
}

fn sample_text() -> PageText {
    let bounds = CharBox {
        left: 1.0,
        top: 2.0,
        right: 3.5,
        bottom: 4.25,
    };
    let chars = "Hé".chars().map(|ch| TextChar { ch, bounds }).collect();
    PageText::new(612.0, 792.0, chars).unwrap()
}

#[test]
fn every_request_survives_a_round_trip() {
    let requests = [
        Request::Open {
            path: "C:\\docs\\ünïcode.pdf".into(),
            password: Some("secret".into()),
        },
        Request::Open {
            path: "/tmp/a.pdf".into(),
            password: None,
        },
        Request::PageCount,
        Request::Render(RenderRequest {
            page: 3,
            width: 800,
            rotation: Rotation::ThreeQuarters,
        }),
        Request::Text { page: 7 },
        Request::Close,
    ];
    for request in &requests {
        assert_eq!(&round_trip_request(request), request);
    }
}

#[test]
fn every_response_survives_a_round_trip() {
    let responses = [
        Response::Opened { pages: 12 },
        Response::PageCount(12),
        Response::Page(Bitmap::new(2, 3, vec![7; 24]).unwrap()),
        Response::Text(sample_text()),
        Response::Closed,
        Response::Failed(PdfError::NeedsPassword),
        Response::Failed(PdfError::NoSuchPage { page: 9, count: 2 }),
        Response::Failed(PdfError::NoEngine("PDF view needs pdfium".into())),
    ];
    for response in &responses {
        assert_eq!(&round_trip_response(response), response);
    }
}

#[test]
fn a_clean_end_of_stream_reads_as_none() {
    assert!(
        read_request(&mut Cursor::new(Vec::new()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_truncated_header_is_rejected() {
    let error = read_frame(&mut Cursor::new(vec![5, 0]), 64).unwrap_err();
    assert!(matches!(error, CodecError::Truncated));
}

#[test]
fn a_truncated_payload_is_rejected() {
    let mut wire = Vec::new();
    write_request(&mut wire, &Request::Text { page: 1 }).unwrap();
    wire.pop();
    let error = read_request(&mut Cursor::new(wire)).unwrap_err();
    assert!(matches!(error, CodecError::Truncated));
}

#[test]
fn an_oversized_frame_is_rejected_before_its_payload_is_read() {
    let wire = (MAX_REQUEST_FRAME as u32 + 1).to_le_bytes().to_vec();
    let error = read_request(&mut Cursor::new(wire)).unwrap_err();
    assert!(matches!(error, CodecError::TooLarge { what: "frame", .. }));
}

#[test]
fn a_path_over_its_limit_is_refused_on_both_sides() {
    let path = "a".repeat(MAX_PATH_BYTES + 1);
    let open = Request::Open {
        path,
        password: None,
    };
    assert!(matches!(
        encode_request(&open).unwrap_err(),
        CodecError::TooLarge { what: "path", .. }
    ));
    let mut payload = vec![1];
    payload.extend_from_slice(&(MAX_PATH_BYTES as u32 + 1).to_le_bytes());
    assert!(matches!(
        decode_request(&payload).unwrap_err(),
        CodecError::TooLarge { what: "path", .. }
    ));
}

#[test]
fn a_password_over_its_limit_is_refused() {
    let open = Request::Open {
        path: "a.pdf".into(),
        password: Some("p".repeat(MAX_PASSWORD_BYTES + 1)),
    };
    assert!(encode_request(&open).is_err());
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut payload = encode_request(&Request::Close).unwrap();
    payload.push(0);
    assert!(matches!(
        decode_request(&payload).unwrap_err(),
        CodecError::Malformed("trailing bytes")
    ));
}

#[test]
fn an_unknown_tag_is_rejected() {
    assert!(decode_request(&[99]).is_err());
    assert!(decode_response(&[99]).is_err());
}

#[test]
fn a_bitmap_whose_length_disagrees_with_its_size_is_rejected() {
    assert!(Bitmap::new(2, 2, vec![0; 15]).is_err());
    let mut payload = vec![3];
    payload.extend_from_slice(&2u32.to_le_bytes());
    payload.extend_from_slice(&2u32.to_le_bytes());
    payload.extend_from_slice(&[0; 12]);
    assert!(matches!(
        decode_response(&payload).unwrap_err(),
        CodecError::Truncated
    ));
}

#[test]
fn a_bitmap_over_the_pixel_limit_is_rejected_without_allocating() {
    let mut payload = vec![3];
    payload.extend_from_slice(&MAX_SIDE.to_le_bytes());
    payload.extend_from_slice(&MAX_SIDE.to_le_bytes());
    assert!(matches!(
        decode_response(&payload).unwrap_err(),
        CodecError::TooLarge { what: "bitmap", .. }
    ));
}

#[test]
fn a_text_count_over_its_limit_is_rejected() {
    let mut payload = vec![4];
    payload.extend_from_slice(&1f32.to_le_bytes());
    payload.extend_from_slice(&1f32.to_le_bytes());
    payload.extend_from_slice(&(MAX_TEXT_CHARS as u32 + 1).to_le_bytes());
    assert!(matches!(
        decode_response(&payload).unwrap_err(),
        CodecError::TooLarge { what: "text", .. }
    ));
}

#[test]
fn a_non_finite_character_box_is_rejected() {
    let bounds = CharBox {
        left: f32::NAN,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    assert!(PageText::new(1.0, 1.0, vec![TextChar { ch: 'a', bounds }]).is_err());
}

#[test]
fn a_surrogate_code_point_is_rejected() {
    let mut payload = vec![4];
    payload.extend_from_slice(&1f32.to_le_bytes());
    payload.extend_from_slice(&1f32.to_le_bytes());
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&0xD800u32.to_le_bytes());
    payload.extend_from_slice(&[0; 16]);
    assert!(matches!(
        decode_response(&payload).unwrap_err(),
        CodecError::Malformed("invalid character")
    ));
}

#[test]
fn text_joins_its_characters() {
    assert_eq!(sample_text().text(), "Hé");
}
