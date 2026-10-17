use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use rusty_cache::{CacheKeys, CachedListing, DriveIdentity, ListingCache};
use rusty_core::{Command, FsErrorKind, PaneId, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use super::drive_fs::{Rig, Setup, drive_window, local};
use crate::format;

const READ_AT_MS: i64 = 1_791_920_400_000;

fn stalled(cx: &mut TestAppContext) -> (Rig, PaneId) {
    let setup = Setup {
        slow: Some("D/slow"),
        ..Setup::default()
    };
    let rig = drive_window(cx, setup);
    let pane = rig.focused(cx);
    (rig, pane)
}

fn enter(rig: &Rig, cx: &mut TestAppContext, path: &str) {
    rig.view.update(cx, |view, cx| {
        let location = local(path);
        let _ = view.browser.command(&Command::PaneNavigate { location });
        cx.notify();
    });
    cx.run_until_parked();
}

fn enter_slow(rig: &Rig, cx: &mut TestAppContext) {
    enter(rig, cx, r"D:\slow");
}

fn wait(cx: &mut TestAppContext, seconds: u64) {
    cx.executor().advance_clock(Duration::from_secs(seconds));
    cx.run_until_parked();
}

fn is_offline(rig: &Rig, cx: &mut TestAppContext, pane: PaneId) -> bool {
    rig.frame(cx, |window, _| {
        window
            .try_find(("waiting", pane.get()))
            .and_then(|waiting| waiting.label().map(str::to_owned))
            .is_some_and(|label| label.ends_with("is not answering · retrying"))
    })
}

fn status(rig: &Rig, cx: &mut TestAppContext, pane: PaneId) -> String {
    rig.frame(cx, |window, _| {
        window
            .find(("status", pane.get()))
            .label()
            .map(str::to_owned)
            .unwrap_or_default()
    })
}

fn has_listing(rig: &Rig, cx: &mut TestAppContext) -> bool {
    rig.view.read_with(cx, |view, _| {
        view.browser.state().active_tab().listing().is_some()
    })
}

fn fail_first_then_answer(rig: &Rig, cx: &mut TestAppContext) {
    rig.fs.fail_list("D/slow", 1, FsErrorKind::NetworkLost);
    rig.fs.release();
    rig.settle_until(cx, |view| !view.browser.loading());
}

fn cache_slow_folder(rig: &Rig, cx: &mut TestAppContext) {
    let keys = CacheKeys::new(Arc::new(|_| Some(DriveIdentity::Serial(7))));
    let deadline = Instant::now() + Duration::from_secs(5);
    let key = keys
        .key_within(&local(r"D:\slow"), deadline)
        .expect("drive D answers");
    let cache = ListingCache::default();
    let row = FsEntry {
        name: ProviderPath::from_text("old.txt"),
        kind: EntryKind::File,
        size: 1,
        modified_ms: None,
        attributes: Attributes::default(),
    };
    let listing = CachedListing {
        rows: vec![row].into(),
        read_at: UNIX_EPOCH + Duration::from_millis(READ_AT_MS.unsigned_abs()),
        folder_changed: None,
        complete: true,
    };
    cache.put(key, listing);
    rig.view
        .update(cx, |view, _| view.browser.set_listing_cache(cache, keys));
}

#[gpui_kit::test]
fn enter_retries_a_folder_whose_last_attempt_failed(cx: &mut TestAppContext) {
    let (rig, pane) = stalled(cx);
    enter_slow(&rig, cx);
    wait(cx, 2);
    fail_first_then_answer(&rig, cx);
    assert!(is_offline(&rig, cx, pane));
    rig.frame(cx, |window, cx| window.press("enter", cx));
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some()
    });
    assert!(!is_offline(&rig, cx, pane));
    assert_eq!(rig.location(cx, pane), local(r"D:\slow"));
}

#[gpui_kit::test]
fn the_retry_timer_lists_again_after_two_seconds(cx: &mut TestAppContext) {
    let (rig, pane) = stalled(cx);
    enter_slow(&rig, cx);
    wait(cx, 2);
    fail_first_then_answer(&rig, cx);
    assert!(!has_listing(&rig, cx));
    wait(cx, 2);
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some()
    });
    assert!(has_listing(&rig, cx));
    assert!(!is_offline(&rig, cx, pane));
}

#[gpui_kit::test]
fn a_cached_folder_that_stalls_says_offline_with_the_read_time(cx: &mut TestAppContext) {
    let (rig, pane) = stalled(cx);
    cache_slow_folder(&rig, cx);
    enter_slow(&rig, cx);
    assert!(status(&rig, cx, pane).contains(" · cached · "));
    wait(cx, 2);
    let offline = format!(
        "Offline · showing cached from {}",
        format::read_time(READ_AT_MS)
    );
    assert!(status(&rig, cx, pane).contains(&offline));
    assert!(rig.frame(cx, |window, _| {
        window.try_find(("waiting", pane.get())).is_none()
    }));
    rig.fs.release();
    rig.settle_until(cx, |view| {
        view.browser
            .state()
            .active_tab()
            .pending_request()
            .is_none()
    });
    assert!(!status(&rig, cx, pane).contains("Offline"));
}

#[gpui_kit::test]
fn backspace_leaves_a_folder_that_is_not_answering(cx: &mut TestAppContext) {
    let (rig, pane) = stalled(cx);
    enter_slow(&rig, cx);
    wait(cx, 2);
    assert!(is_offline(&rig, cx, pane));
    rig.frame(cx, |window, cx| window.press("backspace", cx));
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some()
    });
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    rig.fs.release();
}

#[gpui_kit::test]
fn a_network_error_at_once_goes_offline_and_retries(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = rig.focused(cx);
    rig.fs.fail_list("D/work", 1, FsErrorKind::NetworkLost);
    enter(&rig, cx, r"D:\work");
    rig.settle_until(cx, |view| !view.browser.loading());
    assert!(is_offline(&rig, cx, pane));
    wait(cx, 2);
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some()
    });
    assert!(!is_offline(&rig, cx, pane));
    assert_eq!(rig.location(cx, pane), local(r"D:\work"));
}
