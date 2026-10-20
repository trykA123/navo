use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext, px};

use super::pdf::{LONG_TITLE, Rig, bounds, keys, rig, shown, text};
use super::pdf_links::pdf_of;
use crate::window_view::tests::settle;

fn row(index: usize) -> (&'static str, usize) {
    ("pdf-outline-row", index)
}

fn open_outline(cx: &mut TestAppContext, rig: &Rig) {
    keys(cx, rig, &["ctrl-b"]);
    cx.update_window(rig.0, |_, window, cx| window.click(1usize, cx))
        .expect("window");
    settle(cx, &rig.1);
    keys(cx, rig, &[]);
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> String {
    text(cx, rig.0, "pdf-status")
}

fn is_current(cx: &mut TestAppContext, window: AnyWindowHandle, index: usize) -> bool {
    cx.update_window(window, |_, window, _| {
        window.find(row(index)).selected() == Some(true)
    })
    .expect("window")
}

fn indent(cx: &mut TestAppContext, rig: &Rig, index: usize) -> f32 {
    let title = bounds(cx, rig.0, ("pdf-outline-title", index));
    let first = bounds(cx, rig.0, ("pdf-outline-title", 0usize));
    f32::from(title.origin.x - first.origin.x)
}

#[gpui_kit::test]
fn items_show_their_level_and_one_based_page(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    open_outline(cx, &rig);
    assert_eq!(text(cx, rig.0, row(0)), "Introduction");
    assert_eq!(text(cx, rig.0, row(4)), LONG_TITLE);
    assert_eq!(text(cx, rig.0, ("pdf-outline-page", 0usize)), "1");
    assert_eq!(text(cx, rig.0, ("pdf-outline-page", 1usize)), "3");
    assert_eq!(text(cx, rig.0, ("pdf-outline-page", 3usize)), "");
    assert_eq!(indent(cx, &rig, 1), 12.);
    assert_eq!(indent(cx, &rig, 2), 0.);
    assert_eq!(indent(cx, &rig, 6), 72.);
    let page = bounds(cx, rig.0, ("pdf-outline-page", 1usize));
    let edge = bounds(cx, rig.0, row(1));
    assert!(edge.right() - page.right() < px(24.));
}

#[gpui_kit::test]
fn enter_goes_to_the_page_and_go_back_returns(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    open_outline(cx, &rig);
    keys(cx, &rig, &["down", "down", "enter"]);
    assert_eq!(status(cx, &rig), "PDF · page 10 of 40");
    pdf_of(cx, &rig).update(cx, |pdf, cx| pdf.go_back(cx));
    assert_eq!(status(cx, &rig), "PDF · page 1 of 40");
}

#[gpui_kit::test]
fn a_click_on_an_item_follows_it(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    open_outline(cx, &rig);
    cx.update_window(rig.0, |_, window, cx| window.click(row(5), cx))
        .expect("window");
    settle(cx, &rig.1);
    assert_eq!(status(cx, &rig), "PDF · page 31 of 40");
}

#[gpui_kit::test]
fn the_current_chapter_follows_scrolling(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    open_outline(cx, &rig);
    assert!(is_current(cx, rig.0, 0));
    assert!(!is_current(cx, rig.0, 1));
    keys(
        cx,
        &rig,
        &["ctrl-b", "pagedown", "pagedown", "pagedown", "ctrl-b"],
    );
    assert_eq!(status(cx, &rig), "PDF · page 4 of 40");
    assert!(is_current(cx, rig.0, 1));
    assert!(!is_current(cx, rig.0, 0));
    keys(cx, &rig, &["ctrl-b", "end", "ctrl-b"]);
    assert!(is_current(cx, rig.0, 6));
}

#[gpui_kit::test]
fn a_target_that_is_not_followed_does_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    open_outline(cx, &rig);
    keys(cx, &rig, &["down", "down", "down", "enter"]);
    assert_eq!(status(cx, &rig), "PDF · page 1 of 40");
    pdf_of(cx, &rig).update(cx, |pdf, cx| pdf.go_back(cx));
    assert_eq!(status(cx, &rig), "PDF · page 1 of 40");
}

#[gpui_kit::test]
fn an_outline_that_times_out_once_is_read_on_the_retry(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-slow.pdf");
    open_outline(cx, &rig);
    assert_eq!(text(cx, rig.0, row(0)), "Introduction");
}

#[gpui_kit::test]
fn an_unreadable_outline_says_so_and_is_not_empty(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-lost.pdf");
    open_outline(cx, &rig);
    assert_eq!(
        text(cx, rig.0, "pdf-outline-note"),
        "Ferrite could not read this PDF's outline."
    );
    assert!(!shown(cx, rig.0, row(0)));
}

#[gpui_kit::test]
fn a_document_without_an_outline_says_so(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-flat.pdf");
    open_outline(cx, &rig);
    assert_eq!(
        text(cx, rig.0, "pdf-outline-note"),
        "This PDF has no outline. Use Pages, or Find to jump to a heading."
    );
    assert!(!shown(cx, rig.0, row(0)));
}
