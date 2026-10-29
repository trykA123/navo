use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_cache::ListingCache;
use rusty_core::{Command, TabId};
use rusty_fs::{EntryKind, FakeFs, FsEntry};

use super::tests::gated::Gated;
use super::tests::{big_fake, browser_with, is_cached, names, settle};
use super::{CachedSorted, Fresh, WorkerSorted};
use crate::browser::Browser;
use crate::browser::sort::{sorted_full, ui_sorts};

const ROWS: usize = 30_000;

fn big_with_docs() -> Arc<FakeFs> {
    let fs = big_fake(ROWS);
    fs.add_folder("home/docs");
    fs
}

fn decoy(browser: &Browser, tab: TabId, request: u64) -> CachedSorted {
    let rows: Arc<[FsEntry]> = vec![FsEntry {
        name: rusty_core::ProviderPath::from_text("decoy.txt"),
        kind: EntryKind::File,
        size: 1,
        modified_ms: None,
        attributes: Default::default(),
    }]
    .into();
    let (key, folders_first) = (browser.sort_key(tab), browser.folders_first);
    let full = sorted_full(&rows, key, folders_first);
    CachedSorted {
        tab,
        request,
        rows,
        sorted: WorkerSorted {
            key,
            folders_first,
            full,
        },
    }
}

fn pending_request(browser: &Browser) -> u64 {
    browser.state().active_tab().pending_request().unwrap()
}

fn wait_for_listing(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while browser.state().active_tab().listing().is_none() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn back_into_held_home(gate: &Arc<Gated>, fs: &Arc<FakeFs>) -> Browser {
    let mut browser = browser_with(gate.clone(), fs, ListingCache::default());
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    browser.sorted.clear();
    ui_sorts::take();
    browser.command(&Command::PaneBack).unwrap();
    browser
}

#[test]
fn a_big_cache_hit_is_sorted_on_the_worker_and_shown_before_the_fresh_read() {
    let fs = big_with_docs();
    let gate = Gated::new(&fs, "home", 2);
    let mut browser = back_into_held_home(&gate, &fs);
    gate.wait_until_held();
    wait_for_listing(&mut browser);
    assert_eq!(names(&browser).len(), ROWS + 1);
    assert!(is_cached(&browser));
    assert_eq!(ui_sorts::take(), 0);
    gate.release();
    settle(&mut browser);
    assert_eq!(names(&browser).len(), ROWS + 1);
    assert_eq!(ui_sorts::take(), 0);
}

#[test]
fn an_early_sorted_listing_is_refused_for_a_wrong_request_or_a_tab_with_rows() {
    let fs = big_with_docs();
    let gate = Gated::new(&fs, "home", 2);
    let mut browser = back_into_held_home(&gate, &fs);
    gate.wait_until_held();
    let tab = browser.state().active_tab().id();
    let request = pending_request(&browser);
    assert!(browser.state().active_tab().listing().is_none());
    let wrong = decoy(&browser, tab, request + 1);
    assert!(!browser.accept_cached_sorted(wrong));
    assert!(browser.state().active_tab().listing().is_none());
    wait_for_listing(&mut browser);
    let shown = names(&browser);
    assert_eq!(shown.len(), ROWS + 1);
    let again = decoy(&browser, tab, request);
    assert!(!browser.accept_cached_sorted(again));
    assert_eq!(names(&browser), shown);
    gate.release();
    settle(&mut browser);
}

#[test]
fn an_unchanged_result_without_rows_is_rebuilt_from_the_cache() {
    let fs = big_with_docs();
    let gate = Gated::new(&fs, "home", 2);
    let mut browser = back_into_held_home(&gate, &fs);
    gate.wait_until_held();
    let tab = browser.state().active_tab().id();
    assert!(browser.state().active_tab().listing().is_none());
    let home = fs.location("home");
    let listing = browser.fresh_listing(tab, &home, Fresh::Unchanged);
    assert_eq!(listing.len(), ROWS as u64 + 1);
    gate.release();
    settle(&mut browser);
}

#[test]
fn a_cancelled_fetch_sends_no_early_listing() {
    let fs = big_with_docs();
    let browser = browser_with(fs.clone(), &fs, ListingCache::default());
    let key = browser.keys.key(&fs.location("home")).unwrap();
    let cached = browser.cache.get(&key).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let early = super::Early {
        cached,
        sort: (browser.sort_key(browser.state().active_tab().id()), true),
        notify: Box::new(move |rows, _| {
            let _ = sender.send(rows.len());
        }),
    };
    let cancel = rusty_fs::CancelToken::new();
    cancel.cancel();
    early.send(&cancel);
    assert!(receiver.try_recv().is_err());
}
