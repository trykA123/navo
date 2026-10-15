mod find;

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, ScrollDelta, TestAppContext, point};
use rusty_core::paging::{ByteSource, HEX_WIDTH, LineIndex, hex_row, hex_rows};

use super::{FileHooks, FileStamp, FileText, OpenAs, Paged, PagedKind, PagedView};
use crate::browser::Operations;
use crate::tokens::Mode;
use crate::window_view::tests::{fake, home_beside_dest, open_options, press, viewer_open};
use crate::window_view::{FerriteWindow, StartOptions};

pub(super) struct Memory {
    bytes: Arc<Vec<u8>>,
    changed: AtomicBool,
}

impl ByteSource for Memory {
    fn len(&self) -> u64 {
        self.bytes.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.bytes.as_slice().read_at(offset, buf)
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

pub(super) struct Setup {
    pub open_as: OpenAs,
    pub bytes: Vec<u8>,
    pub editable: bool,
    pub changed: bool,
    pub index_fails: bool,
}

impl Setup {
    pub(super) fn new(open_as: OpenAs, bytes: Vec<u8>) -> Self {
        Self {
            open_as,
            bytes,
            editable: false,
            changed: false,
            index_fails: false,
        }
    }
}

fn index_run(bytes: Arc<Vec<u8>>, fails: bool) -> super::IndexRun {
    Arc::new(move |cancel, progress| {
        if fails {
            return Err("Cannot read it: gone".to_owned());
        }
        let mut index = LineIndex::new();
        for chunk in bytes.chunks(64 * 1024) {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            index.feed(chunk);
            progress(&index);
        }
        index.finish();
        Ok(Some(index))
    })
}

fn paged_of(bytes: &Arc<Vec<u8>>, setup: &Setup) -> Paged {
    let reopened = bytes.clone();
    Paged {
        source: Arc::new(Memory {
            bytes: bytes.clone(),
            changed: AtomicBool::new(setup.changed),
        }),
        reopen: Arc::new(move || {
            Ok(Box::new(Memory {
                bytes: reopened.clone(),
                changed: AtomicBool::new(false),
            }) as Box<dyn ByteSource + Send>)
        }),
        index: index_run(bytes.clone(), setup.index_fails),
        summary: None,
        editable: setup.editable,
    }
}

fn hooks(setup: Setup) -> FileHooks {
    let bytes = Arc::new(setup.bytes.clone());
    let text = String::from_utf8_lossy(&setup.bytes).into_owned();
    let open_as = setup.open_as;
    FileHooks {
        read: Rc::new(move |_| {
            Ok(FileText {
                stamp: Some(FileStamp {
                    len: text.len() as u64,
                    modified_ms: 1,
                }),
                ..FileText::plain(text.clone())
            })
        }),
        write: Rc::new(|_, _| Ok(None)),
        pick: Rc::new(move |_| Ok(open_as)),
        paged: Rc::new(move |_| Ok(paged_of(&bytes, &setup))),
    }
}

pub(super) fn paged_window(
    cx: &mut TestAppContext,
    setup: Setup,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let options = StartOptions {
        state: home_beside_dest(&fs),
        provider: fs,
        on_save: Rc::new(|_| {}),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: crate::window_view::EditorHooks::none(),
        files: hooks(setup),
        view: None,
    };
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    (window, view)
}

pub(super) fn redraw(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
}

pub(super) fn paged(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Entity<PagedView> {
    view.read_with(cx, |view, _| {
        view.viewer.as_ref().and_then(|viewer| viewer.paged.clone())
    })
    .expect("a paged view is open")
}

pub(super) fn top(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u64 {
    let paged = paged(cx, view);
    paged.read_with(cx, |paged, _| paged.top)
}

fn kind(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<PagedKind> {
    let paged = view.read_with(cx, |view, _| {
        view.viewer.as_ref().and_then(|viewer| viewer.paged.clone())
    })?;
    Some(paged.read_with(cx, |paged, _| paged.kind))
}

fn has_editor(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| {
        view.viewer
            .as_ref()
            .is_some_and(|viewer| viewer.editor.is_some())
    })
}

pub(super) fn label(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: (&'static str, usize),
) -> String {
    cx.update_window(window, |_, window, _| {
        window.find(id).label().unwrap_or_default().to_owned()
    })
    .expect("window")
}

pub(super) fn visible(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) -> bool {
    cx.update_window(window, |_, window, _| {
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

pub(super) fn mixed(len: usize) -> Vec<u8> {
    (0..len).map(|at| (at * 7 % 251) as u8).collect()
}

#[gpui_kit::test]
fn a_binary_pick_opens_the_hex_view_on_the_first_row(cx: &mut TestAppContext) {
    let bytes = mixed(4096);
    let expected = hex_row(0, &bytes[..16], false);
    let (window, view) = paged_window(cx, Setup::new(OpenAs::Hex, bytes));
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    assert_eq!(
        label(cx, window, ("paged-row", 0)),
        format!("{}  {}  {}", expected.offset, expected.hex, expected.ascii)
    );
}

#[gpui_kit::test]
fn ctrl_end_shows_the_last_screen_and_ctrl_home_the_first(cx: &mut TestAppContext) {
    let bytes = mixed(64 * 1024);
    let len = bytes.len() as u64;
    let (window, view) = paged_window(cx, Setup::new(OpenAs::Hex, bytes));
    let rows = paged(cx, &view).read_with(cx, |paged, _| paged.rows);
    press(cx, window, &view, "ctrl-end");
    assert_eq!(top(cx, &view), (hex_rows(len) - rows) * HEX_WIDTH);
    press(cx, window, &view, "ctrl-home");
    assert_eq!(top(cx, &view), 0);
    press(cx, window, &view, "pagedown");
    assert_eq!(top(cx, &view), rows * HEX_WIDTH);
    press(cx, window, &view, "down");
    assert_eq!(top(cx, &view), (rows + 1) * HEX_WIDTH);
}

#[gpui_kit::test]
fn alt_1_from_hex_opens_text_edit_for_a_small_text_file(cx: &mut TestAppContext) {
    let setup = Setup {
        editable: true,
        ..Setup::new(OpenAs::Hex, b"hello\n".to_vec())
    };
    let (window, view) = paged_window(cx, setup);
    press(cx, window, &view, "alt-1");
    assert_eq!(kind(cx, &view), None);
    assert!(has_editor(cx, &view));
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn alt_1_from_hex_opens_paged_text_when_text_edit_cannot(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(OpenAs::Hex, mixed(4096)));
    press(cx, window, &view, "alt-1");
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    assert!(!has_editor(cx, &view));
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn alt_3_with_unsaved_edits_warns_before_switching(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(OpenAs::Text, b"hello\n".to_vec()));
    crate::window_view::tests::type_in(cx, window, &view, "x");
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), None);
    assert!(visible(cx, window, "viewer-message"));
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn a_source_changed_on_disk_says_so(cx: &mut TestAppContext) {
    let setup = Setup {
        changed: true,
        ..Setup::new(OpenAs::Hex, mixed(4096))
    };
    let (window, view) = paged_window(cx, setup);
    press(cx, window, &view, "down");
    assert!(visible(cx, window, "paged-changed"));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn one_wheel_notch_scrolls_three_rows(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(OpenAs::Hex, mixed(64 * 1024)));
    cx.update_window(window, |_, window, cx| {
        window.scroll("paged-body", ScrollDelta::Lines(point(0., -1.)), cx);
    })
    .expect("window");
    redraw(cx, window);
    assert_eq!(top(cx, &view), 3 * HEX_WIDTH);
}
