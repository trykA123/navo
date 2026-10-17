use std::time::{Duration, Instant};

use rusty_cache::ListingCache;
use rusty_core::{Command, FsErrorKind};
use rusty_fs::{FakeOp, Fault};

use super::offline::{has_loaded, into_stalled_docs, is_idle, pending, poll_until};
use super::stalled::Stalled;
use super::{browser, browser_with, fake, into_docs_and_back, names};
use crate::browser::{Browser, Offline};

fn poll_until_changed(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !browser.poll() {
        assert!(Instant::now() < deadline, "nothing changed in time");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_finished_change_does_not_relist_an_offline_tab() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::List,
        2,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let share = Stalled::new(&fs, "home");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    share.stop_answering();
    into_docs_and_back(&mut browser, &fs, || {});
    share.wait_until_held(1);
    let (tab, request) = pending(&browser);
    browser.listing_overdue(tab, request);
    share.answer();
    poll_until(&mut browser, is_idle);
    browser.new_folder("reports").unwrap();
    poll_until_changed(&mut browser);
    assert_eq!(pending(&browser), (tab, request));
    assert!(browser.offline(tab).is_some());
    assert_eq!(names(&browser).len(), 5);
}

#[test]
fn refresh_on_an_offline_tab_retries_the_pending_listing() {
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
    share.answer();
    poll_until(&mut browser, is_idle);
    browser.command(&Command::PaneRefresh).unwrap();
    assert_eq!(pending(&browser), (tab, request));
    poll_until(&mut browser, has_loaded);
    assert_eq!(fs.calls("home/docs", FakeOp::List), 2);
    assert_eq!(browser.offline(tab), None);
}

#[test]
fn an_early_network_error_keeps_the_cached_rows_and_starts_retrying() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::List,
        2,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let mut browser = browser(&fs);
    let key = browser.keys.key(&fs.location("home")).unwrap();
    let read_at = browser.cache.get(&key).unwrap().read_at;
    into_docs_and_back(&mut browser, &fs, || {});
    let (tab, request) = pending(&browser);
    poll_until(&mut browser, is_idle);
    assert_eq!(pending(&browser), (tab, request));
    assert_eq!(names(&browser).len(), 5);
    let cached_at = Some(read_at);
    assert_eq!(browser.offline(tab), Some(Offline { cached_at }));
    assert_eq!(
        browser.take_new_offline(),
        [(tab, request, Duration::from_secs(2))]
    );
    assert!(browser.take_new_offline().is_empty());
    assert_eq!(browser.listing_overdue(tab, request), None);
}

#[test]
fn an_early_timeout_without_a_cache_goes_offline_with_no_rows() {
    let fs = fake();
    fs.fault(
        "home/docs",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::Timeout),
    );
    let mut browser = browser(&fs);
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    let (tab, request) = pending(&browser);
    poll_until(&mut browser, is_idle);
    assert_eq!(pending(&browser), (tab, request));
    assert_eq!(browser.offline(tab), Some(Offline { cached_at: None }));
    assert!(browser.error(tab).is_none());
    assert_eq!(
        browser.retry_offline(tab, request),
        Some(Duration::from_secs(5))
    );
    poll_until(&mut browser, has_loaded);
    assert_eq!(browser.offline(tab), None);
}

#[test]
fn an_unknown_error_while_offline_keeps_the_tab_offline() {
    let fs = fake();
    fs.fault(
        "home/docs",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::Unknown),
    );
    let share = Stalled::new(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    browser.listing_overdue(tab, request);
    share.answer();
    poll_until(&mut browser, is_idle);
    assert_eq!(pending(&browser), (tab, request));
    assert!(browser.offline(tab).is_some());
}

#[test]
fn a_share_that_opens_the_folder_but_sends_no_batch_yet_is_not_offline() {
    let fs = fake();
    let share = Stalled::opening(&fs, "home/docs");
    let mut browser = browser_with(share.clone(), &fs, ListingCache::default());
    into_stalled_docs(&mut browser, &share, &fs);
    let (tab, request) = pending(&browser);
    assert_eq!(browser.listing_overdue(tab, request), None);
    assert_eq!(browser.offline(tab), None);
    share.answer();
    poll_until(&mut browser, has_loaded);
}
