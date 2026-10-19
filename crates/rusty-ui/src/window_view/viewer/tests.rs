mod archive;
mod at_line;
mod can_trace;
mod elf_view;
mod encoding;
mod find;
mod find_all;
mod follow;
mod font;
mod image;
mod key_bar;
mod kind;
mod language;
mod lister;
mod markdown;
mod markdown_split;
mod markdown_table;
mod marks;
mod memory;
mod memory_image;
mod menu;
mod menu_copy;
mod menu_kinds;
mod moves;
mod open;
mod pdf;
mod pdf_find;
mod rebound;
mod save;
mod search;
pub(in crate::window_view::viewer) mod structured;
mod view_parts;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, ScrollDelta, TestAppContext, point};
use rusty_core::paging::{ByteSource, HEX_WIDTH, LineIndex, hex_row, hex_rows};
use rusty_fs::FakeFs;

use super::{EDIT_LIMIT, FileHooks, FileStamp, FileText, Head, Paged, PagedKind, PagedView};
use crate::browser::Operations;
use crate::tokens::Mode;
use crate::window_view::tests::{fake, home_beside_dest, open_options, press, viewer_open};
use crate::window_view::{FerriteWindow, StartOptions};

pub(super) struct Memory {
    bytes: Arc<Vec<u8>>,
    changed: AtomicBool,
    reads: Arc<AtomicUsize>,
}

impl ByteSource for Memory {
    fn len(&self) -> u64 {
        self.bytes.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.bytes.as_slice().read_at(offset, buf)
    }

    fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

pub(super) struct Setup {
    pub bytes: Vec<u8>,
    pub len: Option<u64>,
    pub changed: bool,
    pub index_fails: bool,
}

impl Setup {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            len: None,
            changed: false,
            index_fails: false,
        }
    }

    pub(super) fn huge(bytes: Vec<u8>) -> Self {
        Self {
            len: Some(EDIT_LIMIT + 1),
            ..Self::new(bytes)
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Log {
    pub heads: Arc<Mutex<Vec<String>>>,
    pub written: Arc<Mutex<Vec<String>>>,
    pub reads: Arc<AtomicUsize>,
    pub reopened: Arc<AtomicUsize>,
}

impl Log {
    pub(super) fn heads(&self) -> Vec<String> {
        self.heads.lock().expect("log").clone()
    }

    pub(super) fn written(&self) -> Vec<String> {
        self.written.lock().expect("log").clone()
    }

    pub(super) fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    pub(super) fn reopened_reads(&self) -> usize {
        self.reopened.load(Ordering::Relaxed)
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

fn paged_of(bytes: &Arc<Vec<u8>>, setup: &Setup, log: &Log) -> Paged {
    let (reopened, reopened_reads) = (bytes.clone(), log.reopened.clone());
    Paged {
        source: Arc::new(Memory {
            bytes: bytes.clone(),
            changed: AtomicBool::new(setup.changed),
            reads: log.reads.clone(),
        }),
        reopen: Arc::new(move || {
            Ok(Box::new(Memory {
                bytes: reopened.clone(),
                changed: AtomicBool::new(false),
                reads: reopened_reads.clone(),
            }) as Box<dyn ByteSource + Send>)
        }),
        index: index_run(bytes.clone(), setup.index_fails),
        summary: None,
        problem: None,
        editable: false,
        stamp: Some(FileStamp {
            len: bytes.len() as u64,
            modified_ms: 1,
        }),
    }
}

pub(super) fn hooks(setup: Setup, log: &Log) -> FileHooks {
    let bytes = Arc::new(setup.bytes.clone());
    let whole = bytes.clone();
    let part = bytes.clone();
    let text = String::from_utf8_lossy(&setup.bytes).into_owned();
    let head = Head {
        bytes: setup.bytes.iter().take(8 * 1024).copied().collect(),
        len: setup.len.unwrap_or(setup.bytes.len() as u64),
    };
    let (heads, written, counters) = (log.heads.clone(), log.written.clone(), log.clone());
    FileHooks {
        read: Arc::new(move |_| {
            Ok(FileText {
                stamp: Some(FileStamp {
                    len: text.len() as u64,
                    modified_ms: 1,
                }),
                ..FileText::plain(text.clone())
            })
        }),
        write: Arc::new(move |_, save| {
            written.lock().expect("log").push(save.text.to_owned());
            Ok(None)
        }),
        head: Arc::new(move |location| {
            let name = location
                .name()
                .map(|name| name.to_text())
                .unwrap_or_default();
            heads.lock().expect("log").push(name);
            Ok(head.clone())
        }),
        paged: Arc::new(move |_| Ok(paged_of(&bytes, &setup, &counters))),
        bytes: Arc::new(move |_, _| Ok(whole.as_ref().clone())),
        range: Arc::new(move |_, offset, len| {
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(part.len());
            Ok(part[start..(start + len).min(part.len())].to_vec())
        }),
        ..FileHooks::none()
    }
}

pub(super) fn window_on(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    setup: Setup,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Log) {
    let log = Log::default();
    let files = hooks(setup, &log);
    let (window, view) = window_with(cx, fs, files, super::Kinds::builtin());
    (window, view, log)
}

pub(super) fn window_with(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    files: FileHooks,
    kinds: super::Kinds,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_options(cx, options_with(fs, files, kinds))
}

pub(super) fn options_with(fs: Arc<FakeFs>, files: FileHooks, kinds: super::Kinds) -> StartOptions {
    StartOptions {
        state: home_beside_dest(&fs),
        provider: fs,
        on_save: std::rc::Rc::new(|_| {}),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: crate::window_view::EditorHooks::none(),
        files,
        kinds,
        drives: crate::window_view::DriveHooks::none(),
        app_menu: crate::window_view::AppMenuHooks::none(),
        settings: crate::window_view::SettingsHooks::none(),
        keymap: crate::window_view::KeymapHooks::none(),
        view: None,
        listing_cache: rusty_cache::ListingCache::default(),
        cache_keys: rusty_cache::CacheKeys::default(),
        shell_menu: None,
        file_clipboard: None,
        start_watch: None,
        open: crate::window_view::OpenHooks::none(),
        crash_report: None,
    }
}

pub(super) fn paged_window(
    cx: &mut TestAppContext,
    setup: Setup,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = text_window(cx, setup);
    (window, view)
}

pub(super) fn text_window(
    cx: &mut TestAppContext,
    setup: Setup,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Log) {
    let (window, view, log) = window_on(cx, fake(), setup);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    (window, view, log)
}

pub(super) fn redraw(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
}

fn paged_body(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<Entity<PagedView>> {
    view.read_with(cx, |view, _| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        super::paged::PagedBody::of(body.as_ref()).map(|paged| paged.0.clone())
    })
}

pub(super) fn editor_of(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
) -> Option<Entity<gpui_kit::component::input::EditorState>> {
    view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        let editor = body.as_any().downcast_ref::<super::editor::EditorBody>()?;
        Some(editor.editor(cx))
    })
}

pub(super) fn paged(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Entity<PagedView> {
    paged_body(cx, view).expect("a paged view is open")
}

pub(super) fn top(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u64 {
    let paged = paged(cx, view);
    paged.read_with(cx, |paged, _| paged.top)
}

pub(super) fn kind(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<PagedKind> {
    let paged = paged_body(cx, view)?;
    Some(paged.read_with(cx, |paged, _| paged.kind))
}

pub(super) fn has_editor(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    editor_of(cx, view).is_some()
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
fn a_binary_file_opens_the_hex_view_on_the_first_row(cx: &mut TestAppContext) {
    let bytes = mixed(4096);
    let expected = hex_row(0, &bytes[..16], false);
    let (window, view) = paged_window(cx, Setup::new(bytes));
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
    let (window, view) = paged_window(cx, Setup::new(bytes));
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
fn alt_1_from_hex_opens_the_text_editor_for_a_small_text_file(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    press(cx, window, &view, "alt-1");
    assert_eq!(kind(cx, &view), None);
    assert!(has_editor(cx, &view));
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn alt_1_from_hex_opens_paged_text_when_text_edit_cannot(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(mixed(4096)));
    press(cx, window, &view, "alt-1");
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    assert!(!has_editor(cx, &view));
    press(cx, window, &view, "alt-3");
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
}

#[gpui_kit::test]
fn alt_3_with_unsaved_edits_warns_before_switching(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(b"hello\n".to_vec()));
    press(cx, window, &view, "ctrl-e");
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
        ..Setup::new(mixed(4096))
    };
    let (window, view) = paged_window(cx, setup);
    press(cx, window, &view, "down");
    assert!(visible(cx, window, "paged-changed"));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn one_wheel_notch_scrolls_three_rows(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(mixed(64 * 1024)));
    cx.update_window(window, |_, window, cx| {
        window.scroll("paged-body", ScrollDelta::Lines(point(0., -1.)), cx);
    })
    .expect("window");
    redraw(cx, window);
    assert_eq!(top(cx, &view), 3 * HEX_WIDTH);
}

pub(super) fn press_only(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .expect("window");
}

pub(super) fn text_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &'static str,
) -> String {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
            .unwrap_or_default()
    })
    .expect("window")
}

pub(super) fn editor_text(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| state.value().to_string())
}
