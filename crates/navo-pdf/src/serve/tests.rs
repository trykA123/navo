use std::io::Cursor;

use super::*;
use crate::fake::{FakeEngine, PAGES};
use crate::protocol::{LinkTarget, Rotation, read_response, write_request};

fn exchange(engine: &mut dyn Engine, requests: &[Request]) -> Vec<Response> {
    let mut wire = Vec::new();
    for request in requests {
        write_request(&mut wire, request).unwrap();
    }
    let mut output = Vec::new();
    serve(engine, &mut Cursor::new(wire), &mut output).unwrap();
    let mut replies = Cursor::new(output);
    std::iter::from_fn(|| read_response(&mut replies).unwrap()).collect()
}

fn open(path: &str) -> Request {
    Request::Open {
        path: path.into(),
        password: None,
    }
}

#[test]
fn each_request_gets_one_answer_in_order() {
    let render = Request::Render(RenderRequest {
        page: 1,
        width: 50,
        rotation: Rotation::None,
    });
    let replies = exchange(
        &mut FakeEngine::default(),
        &[
            open("a.pdf"),
            Request::PageCount,
            render,
            Request::Text { page: 2 },
        ],
    );
    assert_eq!(replies[0], Response::Opened { pages: PAGES });
    assert_eq!(replies[1], Response::PageCount(PAGES));
    let Response::Page(bitmap) = &replies[2] else {
        panic!("expected a page, got {:?}", replies[2]);
    };
    assert_eq!((bitmap.width(), bitmap.height()), (50, 100));
    let Response::Text(text) = &replies[3] else {
        panic!("expected text, got {:?}", replies[3]);
    };
    assert_eq!(text.text(), "page 2");
}

#[test]
fn a_sideways_render_swaps_the_page_shape() {
    let render = Request::Render(RenderRequest {
        page: 0,
        width: 200,
        rotation: Rotation::Quarter,
    });
    let replies = exchange(&mut FakeEngine::default(), &[open("a.pdf"), render]);
    let Response::Page(bitmap) = &replies[1] else {
        panic!("expected a page, got {:?}", replies[1]);
    };
    assert_eq!((bitmap.width(), bitmap.height()), (200, 100));
}

#[test]
fn engine_errors_cross_as_failed_responses() {
    let replies = exchange(
        &mut FakeEngine::default(),
        &[open("locked.pdf"), Request::PageCount, open("broken.pdf")],
    );
    assert_eq!(
        replies,
        [
            Response::Failed(PdfError::NeedsPassword),
            Response::Failed(PdfError::NotOpen),
            Response::Failed(PdfError::Corrupt),
        ]
    );
}

#[test]
fn a_render_over_the_pixel_limit_is_refused() {
    let render = Request::Render(RenderRequest {
        page: 0,
        width: 16_000,
        rotation: Rotation::None,
    });
    let replies = exchange(&mut FakeEngine::default(), &[open("a.pdf"), render]);
    assert_eq!(replies[1], Response::Failed(PdfError::TooLarge));
}

#[test]
fn a_missing_page_names_the_count() {
    let replies = exchange(
        &mut FakeEngine::default(),
        &[open("a.pdf"), Request::Text { page: PAGES }],
    );
    assert_eq!(
        replies[1],
        Response::Failed(PdfError::NoSuchPage {
            page: PAGES,
            count: PAGES
        })
    );
}

#[test]
fn close_answers_and_ends_the_loop() {
    let replies = exchange(
        &mut FakeEngine::default(),
        &[open("a.pdf"), Request::Close, Request::PageCount],
    );
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1], Response::Closed);
}

#[test]
fn prepare_answers_keeps_serving_and_forgets_the_open_document() {
    let replies = exchange(
        &mut FakeEngine::default(),
        &[open("a.pdf"), Request::Prepare, Request::PageCount],
    );
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[1], Response::Closed);
    assert_eq!(replies[2], Response::Failed(PdfError::NotOpen));
}

#[test]
fn prepare_reports_a_missing_engine() {
    assert_eq!(
        exchange(&mut Unavailable::new("no pdfium"), &[Request::Prepare]),
        [Response::Failed(PdfError::NoEngine("no pdfium".into()))]
    );
}

#[test]
fn a_garbled_request_is_answered_with_a_protocol_error_and_stops() {
    let mut output = Vec::new();
    let result = serve(
        &mut FakeEngine::default(),
        &mut Cursor::new(vec![1, 0, 0, 0, 99]),
        &mut output,
    );
    assert!(result.is_err());
    let reply = read_response(&mut Cursor::new(output)).unwrap().unwrap();
    assert!(matches!(reply, Response::Failed(PdfError::Protocol(_))));
}

#[test]
fn an_unavailable_engine_says_what_is_missing() {
    let replies = exchange(
        &mut Unavailable::new("PDF view needs pdfium"),
        &[open("a.pdf")],
    );
    assert_eq!(
        replies[0],
        Response::Failed(PdfError::NoEngine("PDF view needs pdfium".into()))
    );
}

#[test]
fn clip_keeps_long_messages_under_the_limit_on_a_char_boundary() {
    let clipped = clip("é".repeat(MAX_MESSAGE_BYTES));
    assert!(clipped.len() <= MAX_MESSAGE_BYTES);
    assert!(clipped.chars().all(|ch| ch == 'é'));
}

#[test]
fn the_fake_engine_serves_page_links_and_page_errors() {
    let replies = exchange(
        &mut FakeEngine::default(),
        &[
            Request::Links { page: 0 },
            open("a.pdf"),
            Request::Links { page: 2 },
            Request::Links { page: PAGES },
        ],
    );
    assert_eq!(replies[0], Response::Failed(PdfError::NotOpen));
    let Response::Links(links) = &replies[2] else {
        panic!("expected links")
    };
    assert_eq!((links.width(), links.height()), (100.0, 200.0));
    assert_eq!(
        links.links()[0].target,
        LinkTarget::Page {
            page: 3,
            top: Some(20.0)
        }
    );
    assert_eq!(
        links.links()[1].target,
        LinkTarget::Uri("https://example.org/page/2".into())
    );
    assert_eq!(links.links()[2].target, LinkTarget::Unsupported);
    assert_eq!(
        replies[3],
        Response::Failed(PdfError::NoSuchPage {
            page: PAGES,
            count: PAGES
        })
    );
}

#[test]
fn an_unavailable_engine_has_no_links() {
    let replies = exchange(
        &mut Unavailable::new("no pdfium"),
        &[Request::Links { page: 0 }],
    );
    assert_eq!(replies, [Response::Failed(PdfError::NotOpen)]);
}

#[test]
fn outlines_require_an_open_document() {
    let replies = exchange(
        &mut FakeEngine::default(),
        &[Request::Outline, open("a.pdf"), Request::Outline],
    );
    assert_eq!(replies[0], Response::Failed(PdfError::NotOpen));
    let Response::Outline(outline) = &replies[2] else {
        panic!("expected an outline")
    };
    assert_eq!(outline.items().len(), 4);
    assert_eq!(
        exchange(&mut Unavailable::new("no pdfium"), &[Request::Outline]),
        [Response::Failed(PdfError::NotOpen)]
    );
}
