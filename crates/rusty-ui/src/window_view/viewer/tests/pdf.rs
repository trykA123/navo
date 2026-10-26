use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Entity, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};
use gpui_kit::{AppContext as _, InputEvent as _};
use rusty_core::Location;
use rusty_pdf::Engine;
use rusty_pdf::fake::{FakeEngine, PASSWORD};
use rusty_pdf::protocol::{
    Bitmap, CharBox, LinkTarget, Outline, OutlineItem, PageLink, PageLinks, PageText, PdfError,
    RenderRequest, TextChar,
};

use super::{redraw, visible, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};
use crate::window_view::viewer::pdf::{OpenPdf, PdfBody, PdfPages};
use crate::window_view::viewer::{FileHooks, Kinds};

pub(super) const PAGES: u32 = 40;
pub(super) const WIDTH: f32 = 600.;
const HEIGHT: f32 = 800.;
pub(super) const CHAR: f32 = 8.;
const LINE: f32 = 14.;
const MUTED: u32 = 5;
const PDF_BYTES: &[u8] = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\n\x00";
const MOTION: std::time::Duration = std::time::Duration::from_millis(600);

#[derive(Clone, Default)]
pub(super) struct Calls {
    pub drawn: Arc<Mutex<Vec<u32>>>,
    pub opens: Arc<Mutex<Vec<Option<String>>>>,
}

impl Calls {
    pub(super) fn drawn(&self) -> Vec<u32> {
        self.drawn.lock().expect("calls").clone()
    }
}

pub(super) fn page_text(page: u32) -> String {
    let mut text = format!("Page {} of the manual\n", page + 1);
    if page % 4 == 1 {
        text.push_str("The volt meter reads volt values\n");
    }
    if page % 10 == 3 {
        text.push_str("undervoltage lockout\n");
    }
    text
}

fn chars_of(text: &str) -> Vec<TextChar> {
    let mut chars = Vec::new();
    for (row, line) in text.split_inclusive('\n').enumerate() {
        let top = 40. + row as f32 * LINE;
        for (column, ch) in line.chars().enumerate() {
            let left = 40. + column as f32 * CHAR;
            let bounds = CharBox {
                left,
                top,
                right: left + CHAR - 1.,
                bottom: top + LINE - 2.,
            };
            chars.push(TextChar { ch, bounds });
        }
    }
    chars
}

pub(super) const LINK_COLUMN: f32 = 40.;

fn link_box(top: f32) -> CharBox {
    side_box(LINK_COLUMN, top)
}

fn side_box(left: f32, top: f32) -> CharBox {
    CharBox {
        left,
        top,
        right: left + 80.,
        bottom: top + 14.,
    }
}

fn book_links() -> Result<PageLinks, PdfError> {
    let uri = |uri: &str| LinkTarget::Uri(uri.to_owned());
    let targets = [
        (
            CharBox {
                left: 40.,
                top: 38.,
                right: 200.,
                bottom: 54.,
            },
            LinkTarget::Page { page: 9, top: None },
        ),
        (
            link_box(100.),
            LinkTarget::Page {
                page: 6,
                top: Some(100.),
            },
        ),
        (link_box(130.), uri("https://example.org/a")),
        (link_box(160.), uri("file:///etc/passwd")),
        (link_box(190.), uri("javascript:alert(1)")),
        (link_box(220.), LinkTarget::Unsupported),
        (side_box(300., 100.), uri("https://example.org/\"a")),
        (side_box(300., 130.), uri("https://example.org/a b")),
        (
            side_box(300., 160.),
            uri("https://example.org,/select,C:\\Windows"),
        ),
        (side_box(300., 190.), uri("-https://example.org")),
        (side_box(300., 220.), uri("https://example.org/\u{7}")),
    ];
    let links = targets
        .into_iter()
        .map(|(bounds, target)| PageLink { bounds, target })
        .collect();
    PageLinks::new(WIDTH, HEIGHT, links).map_err(|_| PdfError::Corrupt)
}

pub(super) const LONG_TITLE: &str = "Electrical characteristics over the full recommended operating temperature range with every optional feature enabled";

fn book_outline() -> Result<Outline, PdfError> {
    let page = |page| LinkTarget::Page { page, top: None };
    let entries = [
        ("Introduction", 0, page(0)),
        ("Scope", 1, page(2)),
        ("Operation", 0, page(9)),
        ("Attachment", 0, LinkTarget::Unsupported),
        (LONG_TITLE, 1, page(20)),
        ("Appendix", 0, page(30)),
        ("Deepest", 9, page(31)),
    ];
    let items = entries
        .into_iter()
        .map(|(title, level, target)| OutlineItem {
            title: title.to_owned(),
            level,
            target,
        })
        .collect();
    Outline::new(items).map_err(|_| PdfError::Corrupt)
}

struct Book {
    pages: u32,
    calls: Calls,
    mute: Option<u32>,
    is_flat: bool,
    outline_fails: u32,
}

impl PdfPages for Book {
    fn page_count(&self) -> u32 {
        self.pages
    }

    fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError> {
        self.calls.drawn.lock().expect("calls").push(request.page);
        let height = (request.width as f32 * HEIGHT / WIDTH).round() as u32;
        let rgba = vec![255; request.width as usize * height as usize * 4];
        Bitmap::new(request.width, height, rgba).map_err(|_| PdfError::TooLarge)
    }

    fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        if self.mute == Some(page) {
            return Err(PdfError::Corrupt);
        }
        PageText::new(WIDTH, HEIGHT, chars_of(&page_text(page))).map_err(|_| PdfError::Corrupt)
    }

    fn links(&mut self, _: u32) -> Result<PageLinks, PdfError> {
        book_links()
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        if self.outline_fails > 0 {
            self.outline_fails -= 1;
            return Err(PdfError::Timeout);
        }
        if self.is_flat {
            return Outline::new(Vec::new()).map_err(|_| PdfError::Corrupt);
        }
        book_outline()
    }
}

struct Fake(FakeEngine);

impl PdfPages for Fake {
    fn page_count(&self) -> u32 {
        self.0.page_count().unwrap_or(0)
    }

    fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError> {
        self.0.render(&request)
    }

    fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        self.0.text(page)
    }

    fn links(&mut self, page: u32) -> Result<PageLinks, PdfError> {
        self.0.links(page)
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        self.0.outline()
    }
}

fn name_of(location: &Location) -> String {
    location
        .name()
        .map(|name| name.to_text())
        .unwrap_or_default()
}

fn open_with(calls: &Calls) -> OpenPdf {
    let calls = calls.clone();
    Arc::new(move |location, password| {
        calls
            .opens
            .lock()
            .expect("calls")
            .push(password.map(str::to_owned));
        let name = name_of(location);
        let books = [
            "a-book.pdf",
            "a-muted.pdf",
            "a-flat.pdf",
            "a-slow.pdf",
            "a-lost.pdf",
        ];
        if books.contains(&name.as_str()) {
            return Ok(Box::new(Book {
                pages: PAGES,
                calls: calls.clone(),
                mute: (name == "a-muted.pdf").then_some(MUTED),
                is_flat: name == "a-flat.pdf",
                outline_fails: match name.as_str() {
                    "a-slow.pdf" => 1,
                    "a-lost.pdf" => 2,
                    _ => 0,
                },
            }) as Box<dyn PdfPages>);
        }
        let mut engine = FakeEngine::default();
        engine.open(&name, password)?;
        Ok(Box::new(Fake(engine)) as Box<dyn PdfPages>)
    })
}

pub(super) fn hooks(open: OpenPdf, bytes: Vec<u8>) -> FileHooks {
    FileHooks {
        pdf: open,
        ..super::hooks(super::Setup::new(bytes), &super::Log::default())
    }
}

pub(super) type Rig = (AnyWindowHandle, Entity<FerriteWindow>, Calls);

pub(super) fn rig_with(cx: &mut TestAppContext, name: &str, open: Option<OpenPdf>) -> Rig {
    rig_bytes(cx, name, open, PDF_BYTES.repeat(64))
}

fn rig_bytes(cx: &mut TestAppContext, name: &str, open: Option<OpenPdf>, bytes: Vec<u8>) -> Rig {
    let fs = fake();
    fs.add_file(&format!("home/{name}"), 4);
    let calls = Calls::default();
    let open = open.unwrap_or_else(|| open_with(&calls));
    let (window, view) = window_with(cx, fs, hooks(open, bytes), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    settle(cx, &view);
    redraw(cx, window);
    (window, view, calls)
}

pub(super) fn rig(cx: &mut TestAppContext, name: &str) -> Rig {
    rig_with(cx, name, None)
}

pub(super) fn is_pdf(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| {
        view.viewer
            .as_ref()
            .and_then(|viewer| viewer.body.as_ref())
            .is_some_and(|body| PdfBody::of(body.as_ref()).is_some())
    })
}

pub(super) fn text_now(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> String {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
            .unwrap_or_default()
    })
    .expect("window")
}

pub(super) fn text(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> String {
    redraw(cx, window);
    text_now(cx, window, id)
}

pub(super) fn shown(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> bool {
    let id = id.into();
    cx.update_window(window, |_, window, _| {
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

pub(super) fn keys(cx: &mut TestAppContext, rig: &Rig, keys: &[&str]) {
    for key in keys {
        press(cx, rig.0, &rig.1, key);
    }
    cx.executor().advance_clock(MOTION);
    redraw(cx, rig.0);
    redraw(cx, rig.0);
}

#[gpui_kit::test]
fn a_pdf_opens_in_the_rich_slot_with_its_page_count(cx: &mut TestAppContext) {
    let (window, view, _) = rig(cx, "a-book.pdf");
    assert!(is_pdf(cx, &view));
    assert_eq!(text(cx, window, "pdf-status"), "PDF · page 1 of 40");
    assert!(visible(cx, window, "pdf-stage"));
    assert!(!visible(cx, window, "pdf-left-drawer"));
    assert!(!visible(cx, window, "pdf-right-drawer"));
    assert!(!visible(cx, window, "pdf-page-map"));
}

#[gpui_kit::test]
fn text_and_hex_stay_on_alt_1_and_alt_3(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["alt-3"]);
    assert!(!is_pdf(cx, &rig.1));
    assert_eq!(
        super::kind(cx, &rig.1),
        Some(crate::window_view::viewer::PagedKind::Hex)
    );
    keys(cx, &rig, &["alt-5"]);
    settle(cx, &rig.1);
    assert!(is_pdf(cx, &rig.1));
}

#[gpui_kit::test]
fn only_pages_near_the_view_are_drawn(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    let first = rig.2.drawn();
    assert!(!first.is_empty());
    assert!(first.iter().all(|page| *page <= 2), "{first:?}");
    keys(cx, &rig, &["end"]);
    settle(cx, &rig.1);
    let drawn = rig.2.drawn();
    assert!(drawn.contains(&(PAGES - 1)), "{drawn:?}");
    assert!(
        drawn.iter().all(|page| *page <= 2 || *page >= PAGES - 3),
        "{drawn:?}"
    );
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 40 of 40");
}

#[gpui_kit::test]
fn page_keys_move_by_page_and_ctrl_g_goes_to_a_page(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["pagedown", "pagedown"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 3 of 40");
    keys(cx, &rig, &["pageup"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 2 of 40");
    keys(cx, &rig, &["end", "home"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 1 of 40");
    keys(cx, &rig, &["ctrl-g"]);
    type_in(cx, rig.0, &rig.1, "17");
    keys(cx, &rig, &["enter"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 17 of 40");
}

#[gpui_kit::test]
fn zoom_keys_fit_show_actual_size_and_step(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    let fit = text(cx, rig.0, "pdf-zoom");
    keys(cx, &rig, &["ctrl-1"]);
    assert_eq!(text(cx, rig.0, "pdf-zoom"), "100 %");
    keys(cx, &rig, &["ctrl-="]);
    assert_eq!(text(cx, rig.0, "pdf-zoom"), "120 %");
    keys(cx, &rig, &["ctrl--", "ctrl--"]);
    assert_eq!(text(cx, rig.0, "pdf-zoom"), "83 %");
    keys(cx, &rig, &["ctrl-0"]);
    assert_eq!(text(cx, rig.0, "pdf-zoom"), fit);
}

#[gpui_kit::test]
fn ctrl_wheel_zooms_and_a_plain_wheel_scrolls(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-1"]);
    cx.update_window(rig.0, |_, window, cx| {
        let centre = window.find("pdf-stage").bounds().center();
        let wheel = ScrollWheelEvent {
            position: centre,
            delta: ScrollDelta::Lines(point(0., 1.)),
            modifiers: Modifiers::control(),
            ..Default::default()
        };
        window.dispatch_event(wheel.to_platform_input(), cx);
    })
    .expect("window");
    assert_eq!(text(cx, rig.0, "pdf-zoom"), "120 %");
    for _ in 0..30 {
        cx.update_window(rig.0, |_, window, cx| {
            window.scroll("pdf-stage", ScrollDelta::Lines(point(0., -3.)), cx);
        })
        .expect("window");
    }
    assert_ne!(text(cx, rig.0, "pdf-status"), "PDF · page 1 of 40");
}

#[gpui_kit::test]
fn a_damaged_pdf_shows_a_refusal_card_and_esc_closes(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-broken.pdf");
    assert!(is_pdf(cx, &rig.1));
    let card = text(cx, rig.0, "pdf-refused");
    assert!(card.starts_with("The file is damaged."), "{card}");
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · not opened");
    assert_eq!(text(cx, rig.0, "pdf-problem"), "The file is damaged.");
    keys(cx, &rig, &["escape"]);
    assert!(!viewer_open(cx, &rig.1));
}

#[gpui_kit::test]
fn a_missing_engine_is_named_on_the_card(cx: &mut TestAppContext) {
    let rig = rig_with(
        cx,
        "a-book.pdf",
        Some(crate::window_view::viewer::pdf::no_pdf()),
    );
    let card = text(cx, rig.0, "pdf-refused");
    assert!(card.starts_with("The PDF engine is missing."), "{card}");
}

#[gpui_kit::test]
fn a_locked_pdf_asks_for_the_password_until_it_is_right(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-locked.pdf");
    assert!(visible(cx, rig.0, "pdf-password"));
    assert_eq!(
        text(cx, rig.0, "pdf-status"),
        "PDF · locked, waiting for the password"
    );
    type_in(cx, rig.0, &rig.1, "guess");
    keys(cx, &rig, &["enter"]);
    settle(cx, &rig.1);
    assert!(text(cx, rig.0, "pdf-password-error").starts_with("Wrong password"));
    type_in(cx, rig.0, &rig.1, PASSWORD);
    keys(cx, &rig, &["enter"]);
    settle(cx, &rig.1);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 1 of 12");
}

#[gpui_kit::test]
fn the_page_map_is_off_until_the_look_turns_it_on(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    assert!(!visible(cx, rig.0, "pdf-page-map"));
    let pdf = rig.1.read_with(cx, |view, _| {
        let body = view.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
        body.and_then(|body| PdfBody::of(body.as_ref()).map(|pdf| pdf.0.clone()))
    });
    let pdf = pdf.expect("a PDF is open");
    pdf.update(cx, |pdf, cx| {
        pdf.set_look(
            crate::window_view::viewer::PdfLook {
                hit_rail: true,
                page_map: true,
            },
            cx,
        );
    });
    assert!(text(cx, rig.0, "pdf-page-map").starts_with("Page map: page 1 of 40"));
}

pub(super) fn bounds(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    redraw(cx, window);
    let id = id.into();
    cx.update_window(window, |_, window, _| window.find(id).bounds())
        .expect("window")
}

#[gpui_kit::test]
fn dragging_over_text_and_ctrl_c_copies_it(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    let page = bounds(cx, rig.0, ("pdf-page", 0usize));
    let scale = f32::from(page.size.width) / WIDTH;
    let at = |column: f32| {
        point(
            page.origin.x + px((40. + column * CHAR + 2.) * scale),
            page.origin.y + px((40. + 5.) * scale),
        )
    };
    cx.update_window(rig.0, |_, window, cx| window.drag(at(0.), at(3.), cx))
        .expect("window");
    keys(cx, &rig, &["ctrl-c"]);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("Page"));
    assert_eq!(text(cx, rig.0, "pdf-note"), "Copied 4 characters");
}

#[gpui_kit::test]
fn an_over_long_password_is_wrong_without_starting_the_helper(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-locked.pdf");
    let opens = rig.2.opens.lock().expect("calls").len();
    let long = "x".repeat(rusty_pdf::protocol::MAX_PASSWORD_BYTES + 1);
    cx.update_window(rig.0, |_, window, cx| window.input(&long, cx))
        .expect("window");
    keys(cx, &rig, &["enter"]);
    settle(cx, &rig.1);
    assert!(text(cx, rig.0, "pdf-password-error").starts_with("Wrong password"));
    assert_eq!(rig.2.opens.lock().expect("calls").len(), opens);
}

#[gpui_kit::test]
fn a_text_file_that_starts_like_a_pdf_falls_back_to_text(cx: &mut TestAppContext) {
    let bytes = b"%PDF-1.4 export notes\nThe exporter writes these lines.\n".to_vec();
    let rig = rig_bytes(cx, "a-broken.pdf", None, bytes);
    assert!(!is_pdf(cx, &rig.1));
    assert!(super::has_editor(cx, &rig.1) || super::kind(cx, &rig.1).is_some());
}

#[gpui_kit::test]
fn a_page_whose_text_fails_is_named_in_the_status_line(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-muted.pdf");
    keys(cx, &rig, &["ctrl-f"]);
    type_in(cx, rig.0, &rig.1, "volt");
    settle(cx, &rig.1);
    let problem = text(cx, rig.0, "pdf-problem");
    assert_eq!(problem, format!("Page {} could not be searched", MUTED + 1));
    let count = text(cx, rig.0, "pdf-find-count");
    assert!(count.ends_with("some pages skipped"), "{count}");
}

#[gpui_kit::test]
fn the_page_focus_ring_follows_the_cursor_only_while_focused(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-b"]);
    assert!(shown(cx, rig.0, ("pdf-page-focus-ring", 0usize)));
    keys(cx, &rig, &["down"]);
    assert!(!shown(cx, rig.0, ("pdf-page-focus-ring", 0usize)));
    assert!(shown(cx, rig.0, ("pdf-page-focus-ring", 1usize)));
    cx.update_window(rig.0, |_, window, cx| window.blur(cx))
        .expect("window");
    keys(cx, &rig, &[]);
    assert!(!shown(cx, rig.0, ("pdf-page-focus-ring", 1usize)));
    assert!(shown(cx, rig.0, ("pdf-page-row", 1usize)));
}
