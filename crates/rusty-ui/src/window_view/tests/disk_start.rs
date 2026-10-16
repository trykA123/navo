use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_cache::{CacheKeys, CachedListing, DiskListings, ListingCache, Warm};
use rusty_core::{AppState, Command, CursorMove, Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::super::{FerriteWindow, StartOptions};
use super::{open_unsettled, settle};
use crate::browser::Operations;
use crate::tokens::Mode;

const PATIENT: Duration = Duration::from_secs(5);
const REMEMBERED: &str = "f31337.txt";

fn row(name: &str, kind: EntryKind) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size: 1,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn warm(cache: &ListingCache, folders: Vec<Location>) -> Warm {
    Warm {
        cache: cache.clone(),
        keys: CacheKeys::default(),
        folders,
        deadline: Instant::now() + PATIENT,
    }
}

fn seed_last_session(db: &Path, folder: &Location) {
    let disk = DiskListings::open(db, warm(&ListingCache::default(), Vec::new()));
    let rows = vec![
        row("sub", EntryKind::Folder),
        row("f00000.txt", EntryKind::File),
        row(REMEMBERED, EntryKind::File),
        row("gone.txt", EntryKind::File),
    ];
    let listing = CachedListing {
        rows: rows.into(),
        read_at: SystemTime::UNIX_EPOCH,
        folder_changed: None,
        complete: true,
    };
    let key = CacheKeys::default().key(folder).unwrap();
    disk.save(key, Arc::new(listing));
    assert!(disk.flush(PATIENT));
}

fn disk_names(db: &Path, folder: &Location) -> Vec<String> {
    let cache = ListingCache::default();
    let _disk = DiskListings::open(db, warm(&cache, vec![folder.clone()]));
    let key = CacheKeys::default().key(folder).unwrap();
    cache.get(&key).map_or_else(Vec::new, |listing| {
        listing
            .rows
            .iter()
            .map(|row| row.name.display().into_owned())
            .collect()
    })
}

fn open_from_disk(
    cx: &mut TestAppContext,
    fs: &Arc<FakeFs>,
    db: &Path,
) -> (AnyWindowHandle, Entity<FerriteWindow>, DiskListings) {
    let big = fs.location("big");
    let cache = ListingCache::default();
    let disk = DiskListings::open(db, warm(&cache, vec![big.clone()]));
    let options = StartOptions {
        state: AppState::new(big),
        provider: fs.clone(),
        on_save: Rc::new(|_| {}),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: super::super::EditorHooks::none(),
        files: super::super::FileHooks::none(),
        kinds: super::super::Kinds::builtin(),
        drives: super::super::DriveHooks::none(),
        app_menu: crate::window_view::AppMenuHooks::none(),
        view: None,
        listing_cache: cache.with_disk(disk.clone()),
        cache_keys: CacheKeys::default(),
        shell_menu: None,
    };
    let (window, view) = open_unsettled(cx, options, gpui_kit::px(1000.));
    (window, view, disk)
}

fn big_folder_now() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("big/sub");
    fs.add_file("big/a-new.txt", 1);
    fs.add_file("big/f00000.txt", 1);
    fs.add_file(&format!("big/{REMEMBERED}"), 1);
    Arc::new(fs)
}

fn row_label(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64, at: usize) -> String {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within(("list", pane));
        list.find(("row", at))
            .label()
            .unwrap_or_default()
            .to_owned()
    })
    .expect("window")
}

fn status(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64) -> String {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find(("status", pane))
            .label()
            .unwrap_or_default()
            .to_owned()
    })
    .expect("window")
}

#[gpui_kit::test]
fn start_draws_the_last_session_listing_from_disk_marked_cached(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("cache.db");
    let fs = big_folder_now();
    seed_last_session(&db, &fs.location("big"));
    let (window, view, _disk) = open_from_disk(cx, &fs, &db);
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane().get());
    assert!(row_label(cx, window, pane, 3).starts_with("gone.txt"));
    assert!(status(cx, window, pane).contains("cached"));
}

#[gpui_kit::test]
fn the_fresh_listing_replaces_the_disk_rows_and_keeps_the_cursor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("cache.db");
    let fs = big_folder_now();
    seed_last_session(&db, &fs.location("big"));
    let (window, view, disk) = open_from_disk(cx, &fs, &db);
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane().get());
    let to = CursorMove::Row(2);
    view.update(cx, |view, _| {
        view.browser
            .command(&Command::PaneCursorMove { to })
            .unwrap();
    });
    settle(cx, &view);
    assert!(row_label(cx, window, pane, 3).starts_with(REMEMBERED));
    assert!(!status(cx, window, pane).contains("cached"));
    let cursor = view.read_with(cx, |view, _| {
        view.browser.state().active_tab().view().cursor
    });
    assert_eq!(cursor, 3);
    assert!(disk.flush(PATIENT));
    assert_eq!(
        disk_names(&db, &fs.location("big")),
        ["sub", "a-new.txt", "f00000.txt", REMEMBERED]
    );
}
