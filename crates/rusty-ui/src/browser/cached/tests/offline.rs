use std::time::{Duration, Instant};

use rusty_cache::ListingCache;
use rusty_core::{Command, FsErrorKind, TabId};
use rusty_fs::{FakeOp, Fault};

use super::stalled::Stalled;
use super::{browser_with, cursor_name, fake, into_docs_and_back, is_cached, names, put_cursor_on};
use crate::browser::offline::retry_delay;
use crate::browser::{Browser, Offline};

pub(super) fn pending(browser: &Browser) -> (TabId, u64) {
    let tab = browser.state().active_tab();
    (
        tab.id(),
        tab.pending_request().expect("a listing is pending"),
    )
}

pub(super) fn poll_until(browser: &mut Browser, done: impl Fn(&Browser) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(browser) && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(done(browser), "condition not met in time");
}

pub(super) fn is_idle(browser: &Browser) -> bool {
    !browser.loading()
}

pub(super) fn has_loaded(browser: &Browser) -> bool {
    browser.state().active_tab().pending_request().is_none()
}

fn marked_names(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab();
    tab.selection()
        .marked()
        .into_iter()
        .filter_map(|row| tab.listing()?.get(row).map(|e| e.name.clone()))
        .collect()
}

pub(super) fn into_stalled_docs(browser: &mut Browser, share: &Stalled, fs: &rusty_fs::FakeFs) {
    share.stop_answering();
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    share.wait_until_held(1);
}

#[test]
fn the_retry_delays_are_two_five_ten_then_every_thirty_seconds() {
    let delays: Vec<u64> = (0..6).map(|n| retry_delay(n).as_secs()).collect();
    assert_eq!(delays, [2, 5, 10, 30, 30, 30]);
}

#[test]
fn a_stalled_share_with_a_cached_listing_shows_it_offline_with_its_read_time() {
    let fs = fake();
    let share = Stalled::new(&fs, "home");
    let cache = ListingCache::default();
    let mut browser = browser_with(share.clone(), &fs, cache.clone());
    let key = browser.keys.key(&fs.location("home")).unwrap();
    let read_at = cache.get(&key).unwrap().read_at;
    share.stop_answering();
    into_docs_and_back(&mut browser, &fs, || {});
    share.wait_until_held(1);
    let (tab, request) = pending(&browser);
    assert!(browser.offline(tab).is_none());
    assert_eq!(
        browser.listing_overdue(tab, request),
        Some(Duration::from_secs(2))
    );
    let cached_at = Some(read_at);
    assert_eq!(browser.offline(tab), Some(Offline { cached_at }));
    assert_eq!(names(&browser).len(), 5);
    share.answer();
}

#[test]
fn a_stalled_folder_without_a_cache_goes_offline_with_no_rows() {
    let fs = fake();
    let share = Stalled::new(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    assert!(browser.listing_overdue(tab, request).is_some());
    assert_eq!(browser.offline(tab), Some(Offline { cached_at: None }));
    assert!(browser.state().active_tab().listing().is_none());
    share.answer();
}

#[test]
fn a_late_answer_applies_the_diff_keeps_cursor_and_marks_and_clears_offline() {
    let fs = fake();
    let share = Stalled::new(&fs, "home");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    put_cursor_on(&mut browser, "c.txt");
    share.stop_answering();
    into_docs_and_back(&mut browser, &fs, || fs.add_file("home/0.txt", 1));
    share.wait_until_held(1);
    let (tab, request) = pending(&browser);
    browser.listing_overdue(tab, request);
    put_cursor_on(&mut browser, "d.txt");
    browser.command(&Command::SelectionToggle).unwrap();
    put_cursor_on(&mut browser, "c.txt");
    share.answer();
    poll_until(&mut browser, has_loaded);
    assert_eq!(
        names(&browser),
        ["docs", "0.txt", "a.txt", "b.txt", "c.txt", "d.txt"]
    );
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
    assert_eq!(marked_names(&browser), ["d.txt"]);
    assert_eq!(browser.offline(tab), None);
    assert!(!is_cached(&browser));
}

#[test]
fn retries_follow_the_backoff_and_wait_for_the_previous_attempt() {
    let fs = fake();
    for call in [1, 2] {
        fs.fault(
            "home/docs",
            FakeOp::List,
            call,
            Fault::Fail(FsErrorKind::NetworkLost),
        );
    }
    let share = Stalled::new(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    assert_eq!(
        browser.listing_overdue(tab, request),
        Some(Duration::from_secs(2))
    );
    assert_eq!(
        browser.retry_offline(tab, request),
        Some(Duration::from_secs(5))
    );
    share.answer();
    poll_until(&mut browser, is_idle);
    assert_eq!(fs.calls("home/docs", FakeOp::List), 1);
    assert!(browser.offline(tab).is_some());
    assert_eq!(
        browser.retry_offline(tab, request),
        Some(Duration::from_secs(10))
    );
    poll_until(&mut browser, is_idle);
    assert_eq!(fs.calls("home/docs", FakeOp::List), 2);
    assert!(browser.offline(tab).is_some());
    assert_eq!(
        browser.retry_offline(tab, request),
        Some(Duration::from_secs(30))
    );
    poll_until(&mut browser, has_loaded);
    assert_eq!(fs.calls("home/docs", FakeOp::List), 3);
    assert_eq!(browser.offline(tab), None);
    assert!(browser.state().active_tab().listing().is_some());
}

#[test]
fn retry_now_lists_again_once_the_last_attempt_has_failed() {
    let fs = fake();
    fs.fault(
        "home/docs",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::Timeout),
    );
    let share = Stalled::new(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    browser.listing_overdue(tab, request);
    assert!(!browser.retry_listing(tab));
    share.answer();
    poll_until(&mut browser, is_idle);
    assert!(browser.retry_listing(tab));
    poll_until(&mut browser, has_loaded);
    assert_eq!(browser.offline(tab), None);
}

#[test]
fn leaving_the_folder_stops_the_retries() {
    let fs = fake();
    fs.fault(
        "home/docs",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let share = Stalled::new(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    browser.listing_overdue(tab, request);
    browser.command(&Command::PaneBack).unwrap();
    share.answer();
    poll_until(&mut browser, is_idle);
    assert_eq!(browser.retry_offline(tab, request), None);
    assert_eq!(browser.offline(tab), None);
    poll_until(&mut browser, is_idle);
    assert_eq!(fs.calls("home/docs", FakeOp::List), 1);
}

#[test]
fn a_slow_listing_that_sends_batches_is_not_marked_offline() {
    let fs = fake();
    fs.add_file("home/docs/report.txt", 1);
    let share = Stalled::trickling(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    assert_eq!(browser.listing_overdue(tab, request), None);
    assert_eq!(browser.offline(tab), None);
    share.answer();
    poll_until(&mut browser, has_loaded);
    assert_eq!(names(&browser), ["report.txt"]);
}

#[test]
fn an_error_that_is_not_the_network_ends_the_offline_state() {
    let fs = fake();
    fs.fault(
        "home/docs",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let share = Stalled::new(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    browser.listing_overdue(tab, request);
    share.answer();
    poll_until(&mut browser, has_loaded);
    assert_eq!(browser.offline(tab), None);
    assert!(browser.error(tab).is_some());
}
