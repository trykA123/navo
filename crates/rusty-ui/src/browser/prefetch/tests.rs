use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::{AppState, Command, CursorMove};
use rusty_fs::{FakeFs, FakeOp, ProviderOps};

use crate::browser::Browser;
use crate::browser::cached::tests::gated::Gated;
use crate::browser::cached::tests::stalled::Stalled;

mod held;

use held::Held;

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for path in [
        "home/alpha/one.txt",
        "home/alpha/two.txt",
        "home/beta/x.txt",
    ] {
        fs.add_file(path, 1);
    }
    fs.add_file("home/gamma/y.txt", 1);
    fs.add_file("home/notes.txt", 1);
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
    while (browser.loading() || browser.prefetching() > 0) && Instant::now() < deadline {
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

fn put_cursor_on(browser: &mut Browser, name: &str) {
    let row = names(browser).iter().position(|n| n == name).unwrap();
    let to = CursorMove::Row(row as u64);
    browser.command(&Command::PaneCursorMove { to }).unwrap();
}

fn rest_on(browser: &mut Browser, name: &str) {
    put_cursor_on(browser, name);
    let target = browser.cursor_folder();
    browser.leave_prefetches(target.as_ref());
    if let Some(location) = target {
        browser.prefetch(location);
    }
}

fn is_cached_at(browser: &Browser, fs: &FakeFs, path: &str) -> bool {
    let key = browser.keys.key(&fs.location(path)).unwrap();
    browser.cache.get(&key).is_some()
}

#[test]
fn a_rested_folder_is_listed_into_the_cache() {
    let fs = fake();
    let mut browser = browser(&fs);
    rest_on(&mut browser, "alpha");
    settle(&mut browser);
    assert!(is_cached_at(&browser, &fs, "home/alpha"));
    assert_eq!(fs.calls("home/alpha", FakeOp::List), 1);
    assert!(!browser.loading());
}

#[test]
fn files_and_cached_folders_are_not_prefetched() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "notes.txt");
    assert_eq!(browser.cursor_folder(), None);
    rest_on(&mut browser, "alpha");
    settle(&mut browser);
    rest_on(&mut browser, "beta");
    rest_on(&mut browser, "alpha");
    settle(&mut browser);
    assert_eq!(fs.calls("home/alpha", FakeOp::List), 1);
}

#[test]
fn entering_a_prefetched_folder_shows_its_rows_before_any_new_listing() {
    let fs = fake();
    let gate = Gated::new(&fs, "home/alpha", 2);
    let mut browser = browser_with(gate.clone(), &fs, ListingCache::default());
    rest_on(&mut browser, "alpha");
    settle(&mut browser);
    let row = browser.state().active_tab().view().cursor;
    assert!(browser.open_row(row));
    assert_eq!(names(&browser), ["one.txt", "two.txt"]);
    assert!(browser.is_cached(browser.state().active_tab().id()));
    gate.release();
    settle(&mut browser);
}

#[test]
fn moving_on_cancels_a_prefetch_that_has_not_finished() {
    let fs = fake();
    let share = Stalled::opening(&fs, "home/alpha");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    share.stop_answering();
    rest_on(&mut browser, "alpha");
    share.wait_until_held(1);
    put_cursor_on(&mut browser, "notes.txt");
    browser.leave_prefetches(None);
    settle(&mut browser);
    assert_eq!(browser.prefetching(), 0);
    assert!(!is_cached_at(&browser, &fs, "home/alpha"));
    share.answer();
}

#[test]
fn a_finished_prefetch_stays_cached_after_the_cursor_moves_on() {
    let fs = fake();
    let mut browser = browser(&fs);
    rest_on(&mut browser, "alpha");
    settle(&mut browser);
    put_cursor_on(&mut browser, "notes.txt");
    browser.leave_prefetches(None);
    assert!(is_cached_at(&browser, &fs, "home/alpha"));
}

#[test]
fn three_quick_rests_read_at_most_two_folders_at_once() {
    let fs = fake();
    let held = Held::new(&fs, &["home/alpha", "home/beta"]);
    let mut browser = browser_with(held.clone(), &fs, ListingCache::default());
    rest_on(&mut browser, "alpha");
    held.wait_until_held(1);
    rest_on(&mut browser, "beta");
    rest_on(&mut browser, "gamma");
    browser.poll();
    assert_eq!(fs.calls("home/gamma", FakeOp::List), 0);
    held.release();
    settle(&mut browser);
    assert!(held.most_reading() <= 2);
    assert_eq!(fs.calls("home/beta", FakeOp::List), 0);
    assert!(is_cached_at(&browser, &fs, "home/gamma"));
}

#[test]
fn a_queued_rest_is_dropped_when_the_cursor_moves_on() {
    let fs = fake();
    let held = Held::new(&fs, &["home/alpha", "home/beta"]);
    let mut browser = browser_with(held.clone(), &fs, ListingCache::default());
    rest_on(&mut browser, "alpha");
    held.wait_until_held(1);
    rest_on(&mut browser, "beta");
    rest_on(&mut browser, "gamma");
    put_cursor_on(&mut browser, "notes.txt");
    browser.leave_prefetches(None);
    held.release();
    settle(&mut browser);
    assert_eq!(fs.calls("home/beta", FakeOp::List), 0);
    assert_eq!(fs.calls("home/gamma", FakeOp::List), 0);
}

#[test]
fn a_running_prefetch_never_delays_a_refresh_of_the_open_folder() {
    let fs = fake();
    let held = Held::new(&fs, &["home/alpha"]);
    let mut browser = browser_with(held.clone(), &fs, ListingCache::default());
    rest_on(&mut browser, "alpha");
    held.wait_until_held(1);
    browser.command(&Command::PaneRefresh).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while browser.loading() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let tab = browser.state().active_tab();
    assert!(tab.pending_request().is_none());
    assert!(browser.offline(tab.id()).is_none());
    assert!(names(&browser).contains(&"alpha".to_owned()));
    held.release();
    settle(&mut browser);
}

#[test]
fn enter_on_a_folder_still_prefetching_lists_it_once() {
    let fs = fake();
    let held = Held::new(&fs, &["home/alpha"]);
    let mut browser = browser_with(held.clone(), &fs, ListingCache::default());
    rest_on(&mut browser, "alpha");
    held.wait_until_held(1);
    let row = browser.state().active_tab().view().cursor;
    assert!(browser.open_row(row));
    let target = browser.cursor_folder();
    browser.leave_prefetches(target.as_ref());
    assert!(browser.loading());
    held.release();
    settle(&mut browser);
    assert_eq!(names(&browser), ["one.txt", "two.txt"]);
    assert_eq!(fs.calls("home/alpha", FakeOp::List), 1);
    assert!(!browser.loading());
}

#[test]
fn a_dead_share_entered_while_prefetching_still_goes_offline() {
    let fs = fake();
    let share = Stalled::new(&fs, "home/alpha");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    share.stop_answering();
    rest_on(&mut browser, "alpha");
    share.wait_until_held(1);
    let row = browser.state().active_tab().view().cursor;
    assert!(browser.open_row(row));
    browser.leave_prefetches(None);
    let tab = browser.state().active_tab();
    let (tab, request) = (tab.id(), tab.pending_request().unwrap());
    assert!(browser.listing_overdue(tab, request).is_some());
    assert!(browser.offline(tab).is_some());
    share.answer();
    settle(&mut browser);
    assert!(browser.offline(tab).is_none());
    assert_eq!(names(&browser), ["one.txt", "two.txt"]);
}

#[test]
fn a_share_already_offline_is_not_prefetched() {
    let fs = fake();
    let share = Stalled::new(&fs, "home");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    share.stop_answering();
    let location = fs.location("home/beta");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    browser.command(&Command::PaneBack).unwrap();
    share.wait_until_held(1);
    let tab = browser.state().active_tab();
    let (tab, request) = (tab.id(), tab.pending_request().unwrap());
    assert!(browser.listing_overdue(tab, request).is_some());
    rest_on(&mut browser, "alpha");
    assert_eq!(browser.prefetching(), 0);
    assert_eq!(fs.calls("home/alpha", FakeOp::List), 0);
    assert!(browser.error(tab).is_none());
    share.answer();
}

#[test]
fn back_stays_warm_while_prefetches_fill_a_small_cache() {
    let fs = fake();
    for index in 0..40 {
        for file in 0..12 {
            fs.add_file(&format!("home/alpha/sub{index:02}/filler-{file:02}.bin"), 1);
        }
    }
    let mut browser = browser_with(fs.clone(), &fs, ListingCache::new(16 * 1024));
    let location = fs.location("home/alpha");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    for index in 0..40 {
        rest_on(&mut browser, &format!("sub{index:02}"));
        settle(&mut browser);
    }
    assert!(browser.cache.stats().evictions > 0);
    assert!(browser.go_up());
    assert!(browser.is_cached(browser.state().active_tab().id()));
    assert_eq!(names(&browser)[0], "alpha");
}

#[test]
fn prefetch_resolves_a_results_folder_under_its_base() {
    use rusty_core::{Entry, Event, Listing, Location, apply_event, list_missing};
    let location = Location::new(r"C:\shown").unwrap();
    let mut state = AppState::new(location.clone());
    list_missing(&mut state);
    let id = state.active_tab().id();
    let request = state.active_tab().pending_request().unwrap();
    apply_event(
        &mut state,
        Event::ListingLoaded {
            tab: id,
            request,
            location,
            listing: Listing::new(vec![Entry {
                name: r"sub\a.txt".to_owned(),
                is_folder: true,
            }]),
        },
    );
    let mut tab = state.active_tab().clone();
    tab.set_base(Some(Location::new(r"C:\base").unwrap()));
    assert_eq!(
        super::cursor_folder(&tab),
        Some(Location::new(r"C:\base\sub\a.txt").unwrap())
    );
}
