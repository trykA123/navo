use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Entity, TestAppContext, point, px};

use super::pdf::{CHAR, LINK_COLUMN, Rig, WIDTH, bounds, keys, rig, text};
use crate::window_view::tests::settle;
use crate::window_view::viewer::PdfView;
use crate::window_view::viewer::pdf::PdfBody;

fn link(index: usize) -> (&'static str, usize) {
    ("pdf-link", index)
}

fn pdf_of(cx: &mut TestAppContext, rig: &Rig) -> Entity<PdfView> {
    rig.1
        .read_with(cx, |view, _| {
            let body = view.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
            body.and_then(|body| PdfBody::of(body.as_ref()).map(|pdf| pdf.0.clone()))
        })
        .expect("a PDF is open")
}

fn click(cx: &mut TestAppContext, rig: &Rig, index: usize) {
    cx.update_window(rig.0, |_, window, cx| window.click(link(index), cx))
        .expect("window");
    settle(cx, &rig.1);
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> String {
    text(cx, rig.0, "pdf-status")
}

#[gpui_kit::test]
fn hovering_a_link_names_its_target(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    assert_eq!(text(cx, rig.0, link(1)), "Page 7");
    assert_eq!(text(cx, rig.0, link(2)), "https://example.org/a");
    assert_eq!(text(cx, rig.0, link(5)), "This link type is not followed");
}

#[gpui_kit::test]
fn a_click_on_a_page_link_goes_there_and_back_returns(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    click(cx, &rig, 1);
    assert_eq!(status(cx, &rig), "PDF · page 7 of 40");
    let pdf = pdf_of(cx, &rig);
    pdf.update(cx, |pdf, cx| pdf.go_back(cx));
    assert_eq!(status(cx, &rig), "PDF · page 1 of 40");
    pdf.update(cx, |pdf, cx| pdf.go_forward(cx));
    assert_eq!(status(cx, &rig), "PDF · page 7 of 40");
}

#[gpui_kit::test]
fn an_https_link_calls_the_opener(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    click(cx, &rig, 2);
    assert_eq!(
        *rig.2.uris.lock().expect("calls"),
        ["https://example.org/a".to_owned()]
    );
}

#[gpui_kit::test]
fn file_and_javascript_links_are_not_opened_and_say_so(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    click(cx, &rig, 3);
    assert_eq!(text(cx, rig.0, "pdf-note"), "Not opened: file links");
    click(cx, &rig, 4);
    assert_eq!(text(cx, rig.0, "pdf-note"), "Not opened: javascript links");
    click(cx, &rig, 5);
    assert!(rig.2.uris.lock().expect("calls").is_empty());
    assert_eq!(status(cx, &rig), "PDF · page 1 of 40");
}

#[gpui_kit::test]
fn dragging_from_a_link_selects_text_and_does_not_follow(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    let page = bounds(cx, rig.0, ("pdf-page", 0usize));
    let scale = f32::from(page.size.width) / WIDTH;
    let at = |column: f32| {
        point(
            page.origin.x + px((LINK_COLUMN + column * CHAR + 2.) * scale),
            page.origin.y + px(45. * scale),
        )
    };
    cx.update_window(rig.0, |_, window, cx| window.drag(at(0.), at(3.), cx))
        .expect("window");
    keys(cx, &rig, &["ctrl-c"]);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("Page"));
    assert_eq!(status(cx, &rig), "PDF · page 1 of 40");
}
