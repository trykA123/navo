use gpui_kit::test::TestWindowExt;
use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_pdf::protocol::PdfError;

use super::pdf::{hooks, text};
use super::{redraw, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, viewer_open};
use crate::window_view::viewer::pdf::{OpenPdf, no_pdf};
use crate::window_view::viewer::{Head, Kinds};

const MOTION: std::time::Duration = std::time::Duration::from_millis(600);
const PDF_BYTES: &[u8] = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\n\x00";

fn engineless(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    engineless_with(cx, no_pdf())
}

fn engineless_with(
    cx: &mut TestAppContext,
    open: OpenPdf,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    fs.add_file("home/a-one.pdf", 4);
    let (window, view) = window_with(cx, fs, hooks(open, PDF_BYTES.repeat(64)), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    settle(cx, &view);
    redraw(cx, window);
    (window, view)
}

fn title(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    view.read_with(cx, |view, _| {
        view.viewer
            .as_ref()
            .map(|viewer| viewer.title.to_string())
            .unwrap_or_default()
    })
}

fn command_center_open(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    cx.executor().advance_clock(MOTION);
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find("command-center").is_some()
    })
    .expect("window")
}

fn viewer_has_focus(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> bool {
    let handle = view.read_with(cx, |view, cx| {
        view.viewer.as_ref().map(|viewer| viewer.focus_target(cx))
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        handle.is_some_and(|handle| handle.is_focused(window))
    })
    .expect("window")
}

#[gpui_kit::test]
fn esc_closes_a_viewer_that_could_not_open_a_pdf(cx: &mut TestAppContext) {
    let (window, view) = engineless(cx);
    assert!(text(cx, window, "pdf-refused").starts_with("The PDF engine is missing."));
    assert!(viewer_has_focus(cx, window, &view));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn next_file_and_mode_keys_work_on_the_refusal(cx: &mut TestAppContext) {
    let (window, view) = engineless(cx);
    assert_eq!(title(cx, &view), "a-one.pdf");
    press(cx, window, &view, "n");
    assert_ne!(title(cx, &view), "a-one.pdf");
    press(cx, window, &view, "p");
    assert_eq!(title(cx, &view), "a-one.pdf");
    press(cx, window, &view, "3");
    redraw(cx, window);
    assert!(viewer_open(cx, &view));
    assert!(text(cx, window, "pdf-refused").is_empty());
}

#[gpui_kit::test]
fn the_command_center_opens_from_the_refusal(cx: &mut TestAppContext) {
    let (window, view) = engineless(cx);
    press(cx, window, &view, "ctrl-k");
    assert!(command_center_open(cx, window));
    press(cx, window, &view, "escape");
    assert!(!command_center_open(cx, window));
    assert!(viewer_open(cx, &view));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn the_card_never_shows_the_engines_raw_error(cx: &mut TestAppContext) {
    let raw = "PDF view needs pdfium: LoadLibraryError(DlOpen { desc: \"libpdfium.so\" })";
    let open: OpenPdf = Arc::new(move |_, _| Err(PdfError::NoEngine(raw.to_owned())));
    let (window, _view) = engineless_with(cx, open);
    let card = text(cx, window, "pdf-refused");
    assert!(card.starts_with("The PDF engine is missing."), "{card}");
    assert!(
        !card.contains("LoadLibraryError") && !card.contains("DlOpen"),
        "{card}"
    );
}

#[gpui_kit::test]
fn stepping_from_another_viewer_onto_a_refused_pdf_keeps_every_key(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_file("home/a-table.csv", 4);
    fs.add_file("home/a-z.pdf", 4);
    let mut files = hooks(no_pdf(), PDF_BYTES.repeat(64));
    let head = files.head.clone();
    files.head = Arc::new(move |location| {
        let name = location
            .name()
            .map(|name| name.to_text())
            .unwrap_or_default();
        if name.ends_with(".csv") || name.ends_with(".txt") {
            return Ok(Head {
                bytes: b"a,b\n1,2\n".to_vec(),
                len: 8,
            });
        }
        head(location)
    });
    let (window, view) = window_with(cx, fs, files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(title(cx, &view), "a-table.csv");
    press(cx, window, &view, "n");
    redraw(cx, window);
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(title(cx, &view), "a-z.pdf");
    assert!(text(cx, window, "pdf-refused").starts_with("The PDF engine is missing."));
    assert!(viewer_has_focus(cx, window, &view));
    press(cx, window, &view, "p");
    assert_eq!(title(cx, &view), "a-table.csv");
    press(cx, window, &view, "n");
    redraw(cx, window);
    settle(cx, &view);
    redraw(cx, window);
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}
