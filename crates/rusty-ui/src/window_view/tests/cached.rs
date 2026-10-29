use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_cache::{CacheKeys, CachedListing, ListingCache};
use rusty_core::{AppState, Command, CursorMove, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry, ProviderOps};

use super::super::{FerriteWindow, StartOptions};
use super::{open_options, settle};
use crate::browser::{GatedFs, Operations};
use crate::tokens::Mode;

const ROWS: usize = 50_000;
const REMEMBERED: &str = "f31337.txt";

fn file(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 1,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn big_listing() -> CachedListing {
    let mut rows = vec![FsEntry {
        kind: EntryKind::Folder,
        ..file("sub")
    }];
    rows.extend((0..ROWS).map(|index| file(&format!("f{index:05}.txt"))));
    CachedListing {
        rows: rows.into(),
        read_at: SystemTime::now(),
        folder_changed: None,
        complete: true,
    }
}

fn open_big(
    cx: &mut TestAppContext,
    cache: &ListingCache,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Arc<FakeFs>) {
    open_big_on(cx, cache, |fs| fs.clone())
}

fn open_big_on(
    cx: &mut TestAppContext,
    cache: &ListingCache,
    provider: impl FnOnce(&Arc<FakeFs>) -> Arc<dyn ProviderOps>,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Arc<FakeFs>) {
    let fs = FakeFs::new();
    fs.add_folder("big/sub");
    fs.add_file("big/f00000.txt", 1);
    fs.add_file(&format!("big/{REMEMBERED}"), 1);
    let fs = Arc::new(fs);
    let options = StartOptions {
        state: AppState::new(fs.location("big")),
        provider: provider(&fs),
        on_save: Rc::new(|_| {}),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: super::super::EditorHooks::none(),
        terminal: super::super::TerminalHooks::none(),
        files: super::super::FileHooks::none(),
        kinds: super::super::Kinds::builtin(),
        drives: super::super::DriveHooks::none(),
        app_menu: crate::window_view::AppMenuHooks::none(),
        settings: crate::window_view::SettingsHooks::none(),
        keymap: crate::window_view::KeymapHooks::none(),
        view: None,
        listing_cache: cache.clone(),
        cache_keys: CacheKeys::default(),
        shell_menu: None,
        file_clipboard: None,
        drop_platform: None,
        start_watch: None,
        open: crate::window_view::OpenHooks::none(),
        crash_report: None,
        search: crate::window_view::SearchHooks::none(),
        rename: None,
        retained: std::sync::Arc::new(crate::browser::NoRetained),
    };
    let (window, view) = open_options(cx, options);
    (window, view, fs)
}

fn command(cx: &mut TestAppContext, view: &Entity<FerriteWindow>, command: &Command) {
    view.update(cx, |view, _| view.browser.command(command).unwrap());
}

fn visit_sub_while_big_grows(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
    fs: &FakeFs,
    cache: &ListingCache,
) {
    let to = CursorMove::Row(2);
    command(cx, view, &Command::PaneCursorMove { to });
    let location = fs.location("big/sub");
    command(cx, view, &Command::PaneNavigate { location });
    settle(cx, view);
    cache.put(
        CacheKeys::default().key(&fs.location("big")).unwrap(),
        big_listing(),
    );
}

#[gpui_kit::test]
fn back_into_a_cached_50k_folder_waits_for_the_worker_sort(cx: &mut TestAppContext) {
    let cache = ListingCache::default();
    let mut held = None;
    let (window, view, fs) = open_big_on(cx, &cache, |fs| {
        let gate = GatedFs::new(fs, "big", 2);
        held = Some(gate.clone());
        gate
    });
    let gate = held.expect("gate");
    visit_sub_while_big_grows(cx, &view, &fs, &cache);
    cx.update_window(window, |_, window, cx| {
        window.press("alt-left", cx);
        window.render_frame(cx);
    })
    .expect("window");
    let shown = view.read_with(cx, |view, _| {
        view.browser.state().active_tab().listing().is_some()
    });
    assert!(!shown);
    gate.wait_until_held();
    view.update(cx, |view, cx| view.wake(cx));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !view.read_with(cx, |view, _| {
        view.browser.state().active_tab().listing().is_some()
    }) && std::time::Instant::now() < deadline
    {
        view.update(cx, |view, cx| view.wake(cx));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within(("list", pane.get()));
        let row = list.find(("row", 31_338usize));
        assert!(row.visible());
        assert!(
            row.label()
                .is_some_and(|label| label.starts_with(REMEMBERED))
        );
        let status = window.find(("status", pane.get()));
        assert!(status.label().is_some_and(|label| label.contains("cached")));
    })
    .expect("window");
    let (rows, cursor) = view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        (tab.listing().map_or(0, |l| l.len()), tab.view().cursor)
    });
    assert_eq!((rows, cursor), (ROWS as u64 + 1, 31_338));
    gate.release();
    settle(cx, &view);
}

#[gpui_kit::test]
fn the_fresh_listing_replaces_the_cached_rows_and_keeps_the_cursor(cx: &mut TestAppContext) {
    let cache = ListingCache::default();
    let (window, view, fs) = open_big(cx, &cache);
    visit_sub_while_big_grows(cx, &view, &fs, &cache);
    command(cx, &view, &Command::PaneBack);
    settle(cx, &view);
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within(("list", pane.get()));
        let row = list.find(("row", 2usize));
        assert!(
            row.label()
                .is_some_and(|label| label.starts_with(REMEMBERED))
        );
        assert!(list.try_find(("row", 3usize)).is_none());
        let status = window.find(("status", pane.get()));
        assert!(
            status
                .label()
                .is_some_and(|label| !label.contains("cached"))
        );
    })
    .expect("window");
}
