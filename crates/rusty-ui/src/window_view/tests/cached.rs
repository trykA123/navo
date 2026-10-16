use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_cache::{CacheKeys, CachedListing, ListingCache};
use rusty_core::{AppState, Command, CursorMove, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::super::{FerriteWindow, StartOptions};
use super::{open_options, settle};
use crate::browser::Operations;
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
    let fs = FakeFs::new();
    fs.add_folder("big/sub");
    fs.add_file("big/f00000.txt", 1);
    fs.add_file(&format!("big/{REMEMBERED}"), 1);
    let fs = Arc::new(fs);
    let options = StartOptions {
        state: AppState::new(fs.location("big")),
        provider: fs.clone(),
        on_save: Rc::new(|_| {}),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: super::super::EditorHooks::none(),
        files: super::super::FileHooks::none(),
        kinds: super::super::Kinds::builtin(),
        drives: super::super::DriveHooks::none(),
        view: None,
        listing_cache: cache.clone(),
        cache_keys: CacheKeys::default(),
        shell_menu: None,
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
fn back_into_a_cached_50k_folder_shows_its_rows_in_the_first_frame(cx: &mut TestAppContext) {
    let cache = ListingCache::default();
    let (window, view, fs) = open_big(cx, &cache);
    visit_sub_while_big_grows(cx, &view, &fs, &cache);
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.press("alt-left", cx);
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
