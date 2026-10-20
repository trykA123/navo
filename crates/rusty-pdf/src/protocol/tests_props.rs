use std::io::Cursor;

use proptest::prelude::*;

use super::*;

fn rotation() -> impl Strategy<Value = Rotation> {
    prop_oneof![
        Just(Rotation::None),
        Just(Rotation::Quarter),
        Just(Rotation::Half),
        Just(Rotation::ThreeQuarters),
    ]
}

fn request() -> impl Strategy<Value = Request> {
    prop_oneof![
        (".{0,64}", proptest::option::of(".{0,16}"))
            .prop_map(|(path, password)| Request::Open { path, password }),
        Just(Request::PageCount),
        (any::<u32>(), any::<u32>(), rotation()).prop_map(|(page, width, rotation)| {
            Request::Render(RenderRequest {
                page,
                width,
                rotation,
            })
        }),
        any::<u32>().prop_map(|page| Request::Text { page }),
        any::<u32>().prop_map(|page| Request::Links { page }),
        Just(Request::Close),
    ]
}

fn finite() -> impl Strategy<Value = f32> {
    -1.0e6f32..1.0e6
}

fn text_char() -> impl Strategy<Value = TextChar> {
    (any::<char>(), finite(), finite(), finite(), finite()).prop_map(
        |(ch, left, top, right, bottom)| TextChar {
            ch,
            bounds: CharBox {
                left,
                top,
                right,
                bottom,
            },
        },
    )
}

fn error() -> impl Strategy<Value = PdfError> {
    prop_oneof![
        Just(PdfError::NeedsPassword),
        Just(PdfError::WrongPassword),
        Just(PdfError::Corrupt),
        Just(PdfError::TooLarge),
        Just(PdfError::Timeout),
        Just(PdfError::HelperDied),
        Just(PdfError::NotOpen),
        ".{0,32}".prop_map(PdfError::NoEngine),
        ".{0,32}".prop_map(PdfError::Unreadable),
        ".{0,32}".prop_map(PdfError::NoHelper),
        ".{0,32}".prop_map(PdfError::Protocol),
        (any::<u32>(), any::<u32>()).prop_map(|(page, count)| PdfError::NoSuchPage { page, count }),
    ]
}

fn page_links() -> impl Strategy<Value = PageLinks> {
    let target = prop_oneof![
        (any::<u32>(), proptest::option::of(finite()))
            .prop_map(|(page, top)| LinkTarget::Page { page, top }),
        ".{0,64}".prop_map(LinkTarget::Uri),
        Just(LinkTarget::Unsupported),
    ];
    let link = (text_char(), target).prop_map(|(text, target)| PageLink {
        bounds: text.bounds,
        target,
    });
    (finite(), finite(), proptest::collection::vec(link, 0..16))
        .prop_map(|(width, height, links)| PageLinks::new(width, height, links).unwrap())
}

fn response() -> impl Strategy<Value = Response> {
    prop_oneof![
        any::<u32>().prop_map(|pages| Response::Opened { pages }),
        any::<u32>().prop_map(Response::PageCount),
        (1u32..8, 1u32..8)
            .prop_flat_map(|(width, height)| {
                let len = (width * height * 4) as usize;
                (
                    Just(width),
                    Just(height),
                    proptest::collection::vec(any::<u8>(), len),
                )
            })
            .prop_map(|(width, height, rgba)| {
                Response::Page(Bitmap::new(width, height, rgba).unwrap())
            }),
        (
            finite(),
            finite(),
            proptest::collection::vec(text_char(), 0..16)
        )
            .prop_map(|(width, height, chars)| Response::Text(
                PageText::new(width, height, chars).unwrap()
            )),
        Just(Response::Closed),
        page_links().prop_map(Response::Links),
        error().prop_map(Response::Failed),
    ]
}

proptest! {
    #[test]
    fn random_page_links_round_trip(links in page_links()) {
        let response = Response::Links(links);
        let mut wire = Vec::new();
        write_response(&mut wire, &response).unwrap();
        prop_assert_eq!(read_response(&mut Cursor::new(wire)).unwrap(), Some(response));
    }

    #[test]
    fn random_requests_round_trip(request in request()) {
        let mut wire = Vec::new();
        write_request(&mut wire, &request).unwrap();
        prop_assert_eq!(read_request(&mut Cursor::new(wire)).unwrap(), Some(request));
    }

    #[test]
    fn random_responses_round_trip(response in response()) {
        let mut wire = Vec::new();
        write_response(&mut wire, &response).unwrap();
        prop_assert_eq!(read_response(&mut Cursor::new(wire)).unwrap(), Some(response));
    }

    #[test]
    fn random_bytes_never_panic_and_valid_decodes_re_encode(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        if let Ok(request) = decode_request(&bytes) {
            prop_assert_eq!(encode_request(&request).unwrap(), bytes.clone());
        }
        if let Ok(response) = decode_response(&bytes) {
            prop_assert_eq!(encode_response(&response).unwrap(), bytes.clone());
        }
        let _ = read_request(&mut Cursor::new(bytes.clone()));
        let _ = read_response(&mut Cursor::new(bytes));
    }

    #[test]
    fn every_cut_of_a_valid_frame_is_rejected(request in request(), cut in any::<prop::sample::Index>()) {
        let mut wire = Vec::new();
        write_request(&mut wire, &request).unwrap();
        let cut = cut.index(wire.len());
        prop_assume!(cut > 0);
        wire.truncate(cut);
        prop_assert!(read_request(&mut Cursor::new(wire)).is_err());
    }
}
