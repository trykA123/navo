use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::{AppState, Command, CursorMove, Direction, FsErrorKind, ProviderPath, apply};
use rusty_fs::{Attributes, EntryKind, FakeFs, FakeOp, Fault, FsEntry, WatchChange};

use super::recheck::Recheck;
use super::{Folder, Refresh};
use crate::browser::{Browser, Message};

const WITHIN: Duration = Duration::from_secs(5);

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new().with_watch();
    fs.add_folder("home/docs");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    Arc::new(fs)
}

fn browser_at(fs: &Arc<FakeFs>, state: AppState) -> Browser {
    let mut browser = Browser::new(state, fs.clone(), Arc::new(|| {}));
    browser.set_listing_cache(ListingCache::default(), CacheKeys::default());
    browser.start();
    settle(&mut browser);
    browser
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    browser_at(fs, AppState::new(fs.location("home")))
}

fn two_panes_at_home(fs: &Arc<FakeFs>) -> Browser {
    let mut state = AppState::new(fs.location("home"));
    let direction = Direction::Right;
    apply(&mut state, &Command::LayoutSplit { direction }).unwrap();
    browser_at(fs, state)
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while browser.loading() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
}

fn wait_until(browser: &mut Browser, within: Duration, done: impl Fn(&Browser) -> bool) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        browser.poll();
        if !browser.loading() && done(browser) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

fn eventually(done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + WITHIN;
    while !done() {
        if Instant::now() > deadline {
            return false;
        }
        std::thread::yield_now();
    }
    true
}

fn is_fallback(browser: &Browser, location: &rusty_core::Location) -> bool {
    matches!(
        browser.watching.folders.get(location),
        Some(Folder::Fallback { .. })
    )
}

fn names(browser: &Browser) -> Vec<String> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn has(browser: &Browser, name: &str) -> bool {
    names(browser).iter().any(|n| n == name)
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

fn switch_focus_away_and_back(browser: &mut Browser) {
    for direction in [Direction::Left, Direction::Right] {
        browser
            .command(&Command::LayoutFocus { direction })
            .unwrap();
    }
}

#[test]
fn a_file_created_in_the_open_folder_appears_on_its_own_and_the_cursor_stays() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "c.txt");
    fs.add_file("home/0.txt", 1);
    assert!(wait_until(&mut browser, WITHIN, |b| has(b, "0.txt")));
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
}

#[test]
fn a_deleted_file_under_the_cursor_moves_the_cursor_to_a_neighbour() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "b.txt");
    fs.remove("home/b.txt");
    assert!(wait_until(&mut browser, WITHIN, |b| !has(b, "b.txt")));
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
}

#[test]
fn a_rename_updates_in_place_and_the_cursor_follows_it() {
    let fs = fake();
    let mut browser = browser(&fs);
    put_cursor_on(&mut browser, "b.txt");
    fs.rename_entry("home/b.txt", "home/z.txt");
    assert!(wait_until(&mut browser, WITHIN, |b| has(b, "z.txt")));
    assert!(!has(&browser, "b.txt"));
    assert_eq!(cursor_name(&browser).as_deref(), Some("z.txt"));
}

#[test]
fn a_small_change_patches_the_cached_listing_without_listing_again() {
    let fs = fake();
    let mut browser = browser(&fs);
    let listed = fs.calls("home", FakeOp::List);
    fs.add_file("home/e.txt", 1);
    assert!(wait_until(&mut browser, WITHIN, |b| has(b, "e.txt")));
    assert_eq!(fs.calls("home", FakeOp::List), listed);
    let key = browser.keys.key(&fs.location("home")).unwrap();
    let cached = browser.cache.get(&key).unwrap();
    assert!(cached.rows.iter().any(|row| row.name.display() == "e.txt"));
}

#[test]
fn a_burst_over_the_patch_limit_lists_the_folder_again() {
    let fs = fake();
    let mut browser = browser(&fs);
    let listed = fs.calls("home", FakeOp::List);
    for index in 0..200 {
        fs.add_file(&format!("home/burst{index:03}.txt"), 1);
    }
    assert!(wait_until(&mut browser, WITHIN, |b| names(b).len() == 205));
    assert!(fs.calls("home", FakeOp::List) > listed);
}

#[test]
fn an_overflow_falls_back_to_a_full_revalidation() {
    let fs = fake();
    let mut browser = browser(&fs);
    let listed = fs.calls("home", FakeOp::List);
    fs.overflow_watchers("home");
    assert!(wait_until(&mut browser, WITHIN, |_| {
        fs.calls("home", FakeOp::List) > listed
    }));
    assert!(browser.is_watched(&fs.location("home")));
}

#[test]
fn a_failed_watcher_revalidates_when_its_pane_gets_focus() {
    let fs = fake();
    let mut browser = two_panes_at_home(&fs);
    fs.fail_watchers("home", FsErrorKind::NetworkLost);
    let home = fs.location("home");
    assert!(wait_until(&mut browser, WITHIN, |b| is_fallback(b, &home)));
    settle(&mut browser);
    assert!(eventually(|| fs.watchers("home") == 0));
    fs.add_file("home/later.txt", 1);
    assert!(!has(&browser, "later.txt"));
    switch_focus_away_and_back(&mut browser);
    assert!(wait_until(&mut browser, WITHIN, |b| has(b, "later.txt")));
}

#[test]
fn a_watch_that_cannot_start_falls_back_to_revalidation() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::Watch,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let mut browser = two_panes_at_home(&fs);
    assert!(!browser.is_watched(&fs.location("home")));
    fs.add_file("home/later.txt", 1);
    assert!(browser.revalidate_unwatched());
    assert!(wait_until(&mut browser, WITHIN, |b| has(b, "later.txt")));
}

#[test]
fn changing_folder_drops_the_old_watcher() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(fs.watchers("home"), 1);
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    assert!(eventually(|| fs.watchers("home") == 0));
    assert_eq!(fs.watchers("home/docs"), 1);
}

#[test]
fn two_panes_on_one_folder_share_one_watcher_and_both_follow_it() {
    let fs = fake();
    let mut browser = two_panes_at_home(&fs);
    assert_eq!(fs.watchers("home"), 1);
    fs.add_file("home/both.txt", 1);
    let shows_both = |b: &Browser| {
        b.open_tabs().iter().all(|(tab, _)| {
            b.state()
                .tab(*tab)
                .and_then(|t| t.listing())
                .is_some_and(|l| l.entries().iter().any(|e| e.name == "both.txt"))
        })
    };
    assert!(wait_until(&mut browser, WITHIN, shows_both));
}

#[test]
fn a_watched_folder_with_an_unchanged_change_time_is_not_listed_again() {
    let fs = fake();
    fs.add_entry(
        "",
        FsEntry {
            name: ProviderPath::from_text("home"),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        },
    );
    let mut browser = two_panes_at_home(&fs);
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    let listed = fs.calls("home", FakeOp::List);
    browser.command(&Command::PaneBack).unwrap();
    settle(&mut browser);
    assert_eq!(fs.calls("home", FakeOp::List), listed);
    assert_eq!(names(&browser).len(), 5);
}

#[test]
fn a_recheck_relists_only_a_folder_that_lost_its_watcher() {
    let fs = fake();
    let mut browser = browser(&fs);
    let home = fs.location("home");
    let listed = fs.calls("home", FakeOp::List);
    browser.sender.send(Message::Recheck(home.clone())).unwrap();
    settle(&mut browser);
    assert_eq!(fs.calls("home", FakeOp::List), listed);
    fs.fail_watchers("home", FsErrorKind::NetworkLost);
    assert!(wait_until(&mut browser, WITHIN, |b| is_fallback(b, &home)));
    let listed = fs.calls("home", FakeOp::List);
    browser.sender.send(Message::Recheck(home)).unwrap();
    browser.poll();
    settle(&mut browser);
    assert_eq!(fs.calls("home", FakeOp::List), listed + 1);
}

#[test]
fn a_recheck_ticks_on_its_interval_until_dropped() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&ticks);
    let recheck = Recheck::start(Duration::from_millis(5), move || {
        counted.fetch_add(1, Ordering::SeqCst);
        true
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while ticks.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    drop(recheck);
    assert!(ticks.load(Ordering::SeqCst) >= 3);
    assert_eq!(Arc::strong_count(&ticks), 1);
}

#[test]
fn a_provider_without_watching_gets_no_watcher_and_no_fallback() {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("home/a.txt", 1);
    let mut browser = browser(&fs);
    assert!(!browser.is_watched(&fs.location("home")));
    assert!(!browser.revalidate_unwatched());
}

#[test]
fn closing_the_browser_stops_its_watchers() {
    let fs = fake();
    let browser = browser(&fs);
    assert_eq!(fs.watchers("home"), 1);
    drop(browser);
    assert!(eventually(|| fs.watchers("home") == 0));
}

#[test]
fn queued_patches_merge_until_they_pass_the_limit() {
    let created =
        |index: usize| WatchChange::Created(ProviderPath::from_text(&format!("f{index}")));
    let part = |from: usize| Refresh::of((from..from + 32).map(created).collect());
    assert!(matches!(part(0).merge(part(32)), Refresh::Patch(changes) if changes.len() == 64));
    assert_eq!(part(0).merge(part(32)).merge(part(64)), Refresh::Full);
    assert_eq!(Refresh::Full.merge(part(0)), Refresh::Full);
    assert_eq!(Refresh::of((0..65).map(created).collect()), Refresh::Full);
}

fn size_of(browser: &Browser, name: &str) -> Option<u64> {
    let tab = browser.state().active_tab().id();
    browser
        .details(tab)
        .iter()
        .find(|entry| entry.name.to_text() == name)
        .map(|entry| entry.size)
}

#[test]
fn a_change_made_while_its_tab_was_in_the_background_shows_when_it_returns() {
    let fs = fake();
    let mut browser = browser(&fs);
    let location = Some(fs.location("home/docs"));
    browser.command(&Command::PaneOpenTab { location }).unwrap();
    settle(&mut browser);
    assert!(eventually(|| fs.watchers("home") == 0));
    fs.add_file("home/while-away.txt", 1);
    browser
        .command(&Command::PaneActivateTab { tab: 0 })
        .unwrap();
    assert!(wait_until(&mut browser, WITHIN, |b| has(
        b,
        "while-away.txt"
    )));
}

#[test]
fn a_thousand_writes_to_one_file_patch_it_without_listing_again() {
    let fs = fake();
    let mut browser = browser(&fs);
    let listed = fs.calls("home", FakeOp::List);
    for size in 2..=1_001 {
        fs.add_file("home/a.txt", size);
    }
    assert!(wait_until(&mut browser, WITHIN, |b| size_of(b, "a.txt") == Some(1_001)));
    assert_eq!(fs.calls("home", FakeOp::List), listed);
}

#[test]
fn the_patch_limit_counts_distinct_names() {
    let same = || WatchChange::Modified(ProviderPath::from_text("grows.log"));
    let writes = || Refresh::of((0..100).map(|_| same()).collect());
    assert!(matches!(writes(), Refresh::Patch(_)));
    assert!(matches!(writes().merge(writes()), Refresh::Patch(_)));
}

#[test]
fn a_short_name_lists_the_folder_again_instead_of_patching() {
    let short = WatchChange::Created(ProviderPath::from_text("LONGFI~1.TXT"));
    assert_eq!(Refresh::of(vec![short]), Refresh::Full);
    let renamed = WatchChange::Renamed {
        from: ProviderPath::from_text("a.txt"),
        to: ProviderPath::from_text("B~1.TXT"),
    };
    assert_eq!(Refresh::of(vec![renamed]), Refresh::Full);
}

#[test]
fn office_temp_files_and_backups_are_patched_not_relisted() {
    for name in [
        "~$report.docx",
        "notes.txt~",
        "a~b.txt",
        "LONGNAME~1234567.TXT",
    ] {
        let change = WatchChange::Created(ProviderPath::from_text(name));
        assert!(
            matches!(Refresh::of(vec![change]), Refresh::Patch(_)),
            "{name}"
        );
    }
}

#[test]
fn a_folder_with_a_queued_refresh_is_listed_even_when_its_change_time_is_unchanged() {
    let fs = fake();
    fs.add_entry(
        "",
        FsEntry {
            name: ProviderPath::from_text("home"),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        },
    );
    let mut browser = two_panes_at_home(&fs);
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    let home = fs.location("home");
    let (other, _) = browser
        .open_tabs()
        .into_iter()
        .find(|(_, at)| *at == home)
        .unwrap();
    browser.watching.queued.insert(other, (home, Refresh::Full));
    let listed = fs.calls("home", FakeOp::List);
    browser.command(&Command::PaneBack).unwrap();
    settle(&mut browser);
    assert!(fs.calls("home", FakeOp::List) > listed);
}

#[test]
fn a_recheck_skips_a_tab_whose_listing_is_still_pending() {
    let fs = fake();
    let mut browser = browser(&fs);
    let home = fs.location("home");
    fs.fail_watchers("home", FsErrorKind::NetworkLost);
    assert!(wait_until(&mut browser, WITHIN, |b| is_fallback(b, &home)));
    let listed = fs.calls("home", FakeOp::List);
    browser.command(&Command::PaneRefresh).unwrap();
    browser.accept_recheck(&home);
    settle(&mut browser);
    assert_eq!(fs.calls("home", FakeOp::List), listed + 1);
}
