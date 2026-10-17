use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use rusty_cache::{CacheKeys, CachedListing, ListingCache};
use rusty_core::{AppState, Command, CursorMove, ProviderPath};
use rusty_fs::{Attributes, CancelToken, EntryKind, FakeFs, FakeOp, FsEntry, ProviderOps};

use super::{Fetch, Fresh, Progress};
use crate::browser::Browser;
use crate::file_list::SortBy;

mod gated;
mod kept;
mod offline;
mod stalled;

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    Arc::new(fs)
}

fn browser_with(provider: Arc<dyn ProviderOps>, fs: &FakeFs, cache: ListingCache) -> Browser {
    let state = AppState::new(fs.location("home"));
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.set_listing_cache(cache, CacheKeys::default());
    browser.start();
    settle(&mut browser);
    browser
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    browser_with(fs.clone(), fs, ListingCache::default())
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while browser.loading() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
}

fn names(browser: &Browser) -> Vec<String> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn cursor_name(browser: &Browser) -> Option<String> {
    let tab = browser.state().active_tab();
    tab.listing()
        .and_then(|l| l.get(tab.view().cursor))
        .map(|e| e.name.clone())
}

fn put_cursor_on(browser: &mut Browser, name: &str) {
    let row = names(browser).iter().position(|n| n == name).unwrap();
    let to = CursorMove::Row(row as u64);
    browser.command(&Command::PaneCursorMove { to }).unwrap();
}

fn into_docs_and_back(browser: &mut Browser, fs: &FakeFs, change: impl FnOnce()) {
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(browser);
    change();
    browser.command(&Command::PaneBack).unwrap();
}

fn is_cached(browser: &Browser) -> bool {
    browser.is_cached(browser.state().active_tab().id())
}

#[test]
fn a_first_visit_lists_as_before_and_stores_the_complete_listing() {
    let fs = fake();
    let browser = browser(&fs);
    assert!(!is_cached(&browser));
    let key = browser.keys.key(&fs.location("home")).unwrap();
    let stored = browser.cache.get(&key).unwrap();
    assert!(stored.complete);
    assert_eq!(stored.rows.len(), 5);
}

#[test]
fn back_shows_the_cached_rows_at_once_and_marks_them_cached() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "c.txt");
    into_docs_and_back(&mut browser, &fs, || fs.add_file("home/e.txt", 1));
    assert_eq!(
        names(&browser),
        ["docs", "a.txt", "b.txt", "c.txt", "d.txt"]
    );
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
    assert!(is_cached(&browser));
}

#[test]
fn the_fresh_listing_is_applied_as_a_diff_and_the_cursor_stays() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "c.txt");
    into_docs_and_back(&mut browser, &fs, || {
        fs.add_file("home/0.txt", 1);
        fs.remove("home/a.txt");
        fs.add_file("home/e.txt", 1);
    });
    settle(&mut browser);
    assert_eq!(
        names(&browser),
        ["docs", "0.txt", "b.txt", "c.txt", "d.txt", "e.txt"]
    );
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
    assert!(!is_cached(&browser));
}

#[test]
fn a_vanished_cursor_name_moves_the_cursor_to_the_next_row() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "b.txt");
    into_docs_and_back(&mut browser, &fs, || fs.remove("home/b.txt"));
    settle(&mut browser);
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
}

#[test]
fn marks_made_on_cached_rows_survive_the_fresh_listing() {
    let fs = fake();
    let mut browser = browser(&fs);
    into_docs_and_back(&mut browser, &fs, || fs.add_file("home/0.txt", 1));
    put_cursor_on(&mut browser, "d.txt");
    browser.command(&Command::SelectionToggle).unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab();
    let marked: Vec<String> = tab
        .selection()
        .marked()
        .into_iter()
        .filter_map(|row| tab.listing()?.get(row).map(|e| e.name.clone()))
        .collect();
    assert_eq!(marked, ["d.txt"]);
}

#[test]
fn going_up_into_a_cached_parent_puts_the_cursor_on_the_folder_left() {
    let fs = fake();
    let mut browser = browser(&fs);
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    assert!(browser.go_up());
    assert!(is_cached(&browser));
    assert_eq!(cursor_name(&browser).as_deref(), Some("docs"));
}

#[test]
fn a_slower_stale_revalidation_never_overwrites_a_newer_one() {
    let fs = fake();
    let gate = gated::Gated::new(&fs, "home", 1);
    let cache = ListingCache::default();
    let keys = CacheKeys::default();
    let key = keys.key(&fs.location("home")).unwrap();
    let fetch = |provider: Arc<dyn ProviderOps>| Fetch {
        provider,
        cache: cache.clone(),
        keys: keys.clone(),
        location: fs.location("home"),
        shown: None,
        touched: None,
        is_watched: false,
        progress: Progress::default(),
    };
    let slow = fetch(gate.clone());
    let older = std::thread::spawn(move || slow.run(&CancelToken::new()));
    gate.wait_until_held();
    fs.add_file("home/new.txt", 1);
    let Ok(Fresh::Rows(newer)) = fetch(fs.clone()).run(&CancelToken::new()) else {
        panic!("the newer listing has rows");
    };
    gate.release();
    assert!(matches!(older.join().unwrap(), Ok(Fresh::Rows(rows)) if rows.len() == 5));
    let stored = cache.get(&key).unwrap();
    assert_eq!(stored.rows, newer);
    assert!(
        stored
            .rows
            .iter()
            .any(|row| row.name.display() == "new.txt")
    );
}

#[test]
fn pins_keep_the_parent_cached_under_budget_pressure() {
    let fs = fake();
    let mut browser = browser_with(fs.clone(), &fs, ListingCache::new(64 * 1024));
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    for index in 0..40 {
        let key = browser
            .keys
            .key(&fs.location(&format!("other/{index}")))
            .unwrap();
        browser.cache.put(key, filler(200));
    }
    assert!(browser.cache.stats().evictions > 0);
    let parent = browser.keys.key(&fs.location("home")).unwrap();
    assert!(browser.cache.get(&parent).is_some());
}

#[test]
fn leaving_a_folder_unpins_it() {
    let fs = fake();
    let cache = ListingCache::default();
    let mut browser = browser_with(fs.clone(), &fs, cache.clone());
    let pinned_at_home = cache.pinned_bytes();
    assert!(pinned_at_home > 0);
    fs.add_folder("away");
    let location = fs.location("away");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    assert!(cache.pinned_bytes() < pinned_at_home);
    drop(browser);
    assert_eq!(cache.pinned_bytes(), 0);
}

#[test]
fn back_always_revalidates_even_an_unchanged_folder() {
    let fs = fake();
    stamp_folder(&fs, 1_000);
    let mut browser = browser(&fs);
    into_docs_and_back(&mut browser, &fs, || {});
    settle(&mut browser);
    assert_eq!(fs.calls("home", FakeOp::List), 2);
    assert!(!is_cached(&browser));
}

#[test]
fn filter_and_sort_work_on_cached_rows_while_the_fresh_listing_is_held() {
    let fs = fake();
    let gate = gated::Gated::new(&fs, "home", 2);
    let mut browser = browser_with(gate.clone(), &fs, ListingCache::default());
    put_cursor_on(&mut browser, "c.txt");
    into_docs_and_back(&mut browser, &fs, || fs.add_file("home/c2.txt", 1));
    gate.wait_until_held();
    assert!(browser.sort_by(SortBy::Name));
    assert_eq!(
        names(&browser),
        ["docs", "d.txt", "c.txt", "b.txt", "a.txt"]
    );
    assert!(browser.filter_push('c'));
    assert_eq!(names(&browser), ["c.txt"]);
    assert!(is_cached(&browser));
    gate.release();
    settle(&mut browser);
    assert_eq!(names(&browser), ["c2.txt", "c.txt"]);
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
    let tab = browser.state().active_tab().id();
    let shown: Vec<String> = browser
        .details(tab)
        .iter()
        .map(|e| e.name.to_text())
        .collect();
    assert_eq!(shown, ["c2.txt", "c.txt"]);
}

#[test]
fn back_reuses_the_sorted_rows_of_the_last_visit() {
    let fs = fake();
    let mut browser = browser(&fs);
    let tab = browser.state().active_tab().id();
    let first = browser.details(tab);
    into_docs_and_back(&mut browser, &fs, || {});
    assert!(Rc::ptr_eq(&first, &browser.details(tab)));
}

#[test]
fn an_unchanged_fresh_listing_keeps_the_shown_rows() {
    let fs = fake();
    let mut browser = browser(&fs);
    let tab = browser.state().active_tab().id();
    into_docs_and_back(&mut browser, &fs, || {});
    let cached = browser.details(tab);
    settle(&mut browser);
    assert!(Rc::ptr_eq(&cached, &browser.details(tab)));
    assert!(!is_cached(&browser));
}

#[test]
fn closing_a_tab_drops_its_rows() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser
        .command(&Command::PaneOpenTab { location: None })
        .unwrap();
    settle(&mut browser);
    let closed = browser.state().active_tab().id();
    assert!(browser.full.contains_key(&closed));
    browser
        .command(&Command::PaneCloseTab { tab: None })
        .unwrap();
    assert!(!browser.full.contains_key(&closed));
    assert!(!browser.details.contains_key(&closed));
    assert!(!browser.cached.contains_key(&closed));
    assert!(!browser.sorted.contains_key(&closed));
}

fn stamp_folder(fs: &FakeFs, modified_ms: i64) {
    fs.add_entry(
        "",
        FsEntry {
            name: ProviderPath::from_text("home"),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: Some(modified_ms),
            attributes: Attributes::default(),
        },
    );
}

fn filler(rows: usize) -> CachedListing {
    CachedListing {
        rows: (0..rows)
            .map(|index| FsEntry {
                name: ProviderPath::from_text(&format!("filler-{index:04}.bin")),
                kind: EntryKind::File,
                size: 0,
                modified_ms: None,
                attributes: Attributes::default(),
            })
            .collect(),
        read_at: SystemTime::now(),
        folder_changed: None,
        complete: true,
    }
}
