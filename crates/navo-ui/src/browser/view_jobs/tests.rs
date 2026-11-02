use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_core::{AppState, Command, CursorMove, Location, TabId};
use navo_fs::{CancelToken, FakeFs, ProviderOps};

use super::{Stamp, Viewed, applied, shown_until, submitted};
use crate::browser::Browser;
use crate::browser::sort::SortBy;

const ROWS: usize = 25_000;
const BIG: u64 = 1_000_000;

fn fake(files: usize) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_folder("home/docs/inner");
    fs.add_file("home/1a.md", BIG);
    fs.add_file("home/ant.md", BIG);
    for index in 0..files {
        fs.add_file(&format!("home/f{index:05}.txt"), (files - index) as u64);
    }
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let state = AppState::new(fs.location("home"));
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    settle(&mut browser);
    submitted();
    applied();
    browser
}

fn big() -> (Arc<FakeFs>, Browser) {
    let fs = fake(ROWS);
    let browser = browser(&fs);
    (fs, browser)
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while (browser.loading() || browser.running()) && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
}

fn wait_views(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !browser.view_jobs.is_idle() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(browser.view_jobs.is_idle(), "a view job never answered");
}

fn names(browser: &Browser) -> Vec<String> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn files_with(text: &str) -> Vec<String> {
    (0..ROWS)
        .map(|index| format!("f{index:05}.txt"))
        .filter(|name| name.contains(text))
        .collect()
}

fn typed(browser: &mut Browser, text: &str) {
    for ch in text.chars() {
        assert!(browser.filter_push(ch));
    }
}

fn active(browser: &Browser) -> TabId {
    browser.state().active_tab().id()
}

fn mark(browser: &mut Browser, name: &str) {
    let row = names(browser).iter().position(|n| n == name).unwrap();
    let to = CursorMove::Row(row as u64);
    browser.command(&Command::PaneCursorMove { to }).unwrap();
    browser.command(&Command::SelectionToggle).unwrap();
}

fn marked(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab();
    let listing = tab.listing().unwrap();
    let mut names: Vec<String> = tab
        .selection()
        .marked()
        .into_iter()
        .filter_map(|row| listing.get(row).map(|e| e.name.clone()))
        .collect();
    names.sort();
    names
}

fn cursor_name(browser: &Browser) -> Option<String> {
    let tab = browser.state().active_tab();
    tab.listing()
        .and_then(|l| l.get(tab.view().cursor))
        .map(|e| e.name.clone())
}

#[test]
fn two_quick_keystrokes_apply_only_the_last_result() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "12");
    assert_eq!(names(&browser).len(), ROWS + 3);
    assert_eq!(browser.filter_text(active(&browser)), Some("12"));
    wait_views(&mut browser);
    assert_eq!(submitted(), 2);
    assert_eq!(applied(), 1);
    assert_eq!(names(&browser), files_with("12"));
}

#[test]
fn a_result_after_navigation_is_dropped() {
    let (fs, mut browser) = big();
    typed(&mut browser, "1");
    let location = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    wait_views(&mut browser);
    assert_eq!(applied(), 0);
    assert_eq!(names(&browser), vec!["inner".to_owned()]);
    assert_eq!(browser.filter_text(active(&browser)), None);
}

#[test]
fn a_result_for_replaced_rows_is_dropped_and_asked_again() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "f00001");
    assert_eq!(submitted(), 6);
    let tab = active(&browser);
    let full = browser.full.get_mut(&tab).unwrap();
    let mut rows = full.details.to_vec();
    let row = rows
        .iter_mut()
        .find(|e| e.name.to_text() == "f00001.txt")
        .unwrap();
    row.size = 777;
    full.details = rows.into();
    wait_views(&mut browser);
    assert_eq!(submitted(), 1);
    assert_eq!(applied(), 1);
    assert_eq!(size_of(&browser, "f00001.txt"), Some(777));
}

#[test]
fn marks_made_while_a_filter_job_is_pending_stay_on_their_names() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "1");
    mark(&mut browser, "f00010.txt");
    mark(&mut browser, "f00002.txt");
    assert_eq!(names(&browser).len(), ROWS + 3);
    wait_views(&mut browser);
    let tab = active(&browser);
    assert_eq!(marked(&browser), vec!["f00010.txt".to_owned()]);
    assert_eq!(browser.hidden_marks(tab), 1);
    assert!(browser.filter_clear());
    wait_views(&mut browser);
    assert_eq!(names(&browser).len(), ROWS + 3);
    assert_eq!(marked(&browser), ["f00002.txt", "f00010.txt"]);
    assert_eq!(browser.hidden_marks(tab), 0);
}

#[test]
fn marks_stay_on_their_names_after_an_async_sort() {
    let (_fs, mut browser) = big();
    mark(&mut browser, "f00010.txt");
    mark(&mut browser, "ant.md");
    assert!(browser.sort_by(SortBy::Size));
    assert_eq!(browser.sort_key(active(&browser)).by, SortBy::Size);
    assert_eq!(names(&browser)[1], "1a.md");
    wait_views(&mut browser);
    assert_eq!(names(&browser)[1], format!("f{:05}.txt", ROWS - 1));
    assert_eq!(marked(&browser), ["ant.md", "f00010.txt"]);
}

#[test]
fn a_keystroke_without_matches_restores_the_previous_text() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "f0");
    wait_views(&mut browser);
    typed(&mut browser, "z");
    assert_eq!(browser.filter_text(active(&browser)), Some("f0z"));
    wait_views(&mut browser);
    assert_eq!(browser.filter_text(active(&browser)), Some("f0"));
    assert_eq!(names(&browser), files_with("f0"));
}

#[test]
fn quick_keystrokes_without_matches_fall_back_to_no_filter() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "zq");
    wait_views(&mut browser);
    assert_eq!(browser.filter_text(active(&browser)), None);
    assert_eq!(names(&browser).len(), ROWS + 3);
    assert_eq!(applied(), 1);
}

#[test]
fn a_sort_change_while_a_filter_job_runs_ends_in_the_right_order() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "9");
    assert!(browser.sort_by(SortBy::Size));
    wait_views(&mut browser);
    let mut expected = files_with("9");
    expected.reverse();
    assert_eq!(names(&browser), expected);
    assert_eq!(browser.filter_text(active(&browser)), Some("9"));
}

#[test]
fn a_filter_keystroke_while_a_sort_job_runs_keeps_the_new_order() {
    let (_fs, mut browser) = big();
    assert!(browser.sort_by(SortBy::Size));
    typed(&mut browser, "9");
    wait_views(&mut browser);
    let mut expected = files_with("9");
    expected.reverse();
    assert_eq!(names(&browser), expected);
}

#[test]
fn the_cursor_lands_on_the_first_match_after_the_result_applies() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "a");
    assert_eq!(cursor_name(&browser).as_deref(), Some("docs"));
    wait_views(&mut browser);
    assert_eq!(names(&browser), ["1a.md", "ant.md"]);
    assert_eq!(cursor_name(&browser).as_deref(), Some("ant.md"));
}

#[test]
fn hiding_hidden_files_while_a_job_runs_asks_again_with_the_new_visibility() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "f0000");
    assert_eq!(submitted(), 5);
    browser.set_show_hidden(false);
    wait_views(&mut browser);
    assert_eq!(submitted(), 1);
    assert_eq!(applied(), 1);
    assert_eq!(names(&browser), files_with("f0000"));
}

#[test]
fn a_folder_at_the_limit_filters_and_sorts_on_the_ui_thread() {
    let fs = fake(20_000 - 3);
    let mut browser = browser(&fs);
    assert_eq!(names(&browser).len(), 20_000);
    typed(&mut browser, "a");
    assert_eq!(names(&browser), ["1a.md", "ant.md"]);
    assert!(browser.filter_clear());
    assert!(browser.sort_by(SortBy::Size));
    assert_eq!(names(&browser)[1], format!("f{:05}.txt", 20_000 - 4));
    assert_eq!(submitted(), 0);
}

#[test]
fn one_row_over_the_limit_runs_on_the_worker() {
    let fs = fake(20_000 - 2);
    let mut browser = browser(&fs);
    typed(&mut browser, "a");
    assert_eq!(names(&browser).len(), 20_001);
    wait_views(&mut browser);
    assert_eq!(names(&browser), ["1a.md", "ant.md"]);
    assert_eq!(submitted(), 1);
}

fn size_of(browser: &Browser, name: &str) -> Option<u64> {
    let details = browser.details(active(browser));
    details
        .iter()
        .find(|e| e.name.to_text() == name)
        .map(|e| e.size)
}

#[test]
fn measured_sizes_during_a_filter_job_apply_without_asking_again() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "d");
    let tab = active(&browser);
    for bytes in [10, 20, 30] {
        browser.show_size_of(tab, "docs", bytes);
    }
    browser.show_size_of(tab, "f00001.txt", 999);
    wait_views(&mut browser);
    assert_eq!(submitted(), 1);
    assert_eq!(applied(), 1);
    assert_eq!(names(&browser), ["docs", "1a.md", "ant.md"]);
    assert_eq!(size_of(&browser, "docs"), Some(30));
    assert!(browser.filter_clear());
    wait_views(&mut browser);
    assert_eq!(size_of(&browser, "docs"), Some(30));
    assert_eq!(size_of(&browser, "f00001.txt"), Some(999));
}

#[test]
fn revealing_a_name_cancels_the_pending_filter_job() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "f0000");
    let tab = active(&browser);
    browser.reveal_name(tab, "zz.md");
    wait_views(&mut browser);
    assert_eq!(applied(), 0);
    assert_eq!(browser.filter_text(tab), None);
    assert_eq!(names(&browser).len(), ROWS + 3);
}

#[test]
fn a_new_file_outside_a_pending_filter_is_shown() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "f0000");
    assert_eq!(browser.new_file("zz.md"), Ok(()));
    settle(&mut browser);
    wait_views(&mut browser);
    settle(&mut browser);
    assert_eq!(browser.filter_text(active(&browser)), None);
    assert!(names(&browser).contains(&"zz.md".to_owned()));
}

#[test]
fn a_pending_keystroke_keeps_the_last_applied_text_as_fallback() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "a");
    wait_views(&mut browser);
    typed(&mut browser, "qr");
    wait_views(&mut browser);
    assert_eq!(browser.filter_text(active(&browser)), Some("a"));
    assert_eq!(names(&browser), ["1a.md", "ant.md"]);
}

fn stale(browser: &Browser, revision: u64, location: Location) -> Viewed {
    let (tab, shows) = (active(browser), browser.visibility());
    let full = browser.full.get(&tab).unwrap();
    let shown = shown_until(full, "", shows, &CancelToken::new()).unwrap();
    let stamp = Stamp {
        tab,
        location,
        revision,
        shows,
    };
    Viewed {
        stamp,
        text: String::new(),
        full: None,
        shown,
    }
}

#[test]
fn a_result_with_an_old_revision_is_refused() {
    let (fs, mut browser) = big();
    typed(&mut browser, "12");
    let old = stale(&browser, browser.view_jobs.next - 1, fs.location("home"));
    assert!(!browser.accept_viewed(old));
    assert_eq!(applied(), 0);
    assert!(!browser.view_jobs.is_idle());
    wait_views(&mut browser);
    assert_eq!(names(&browser), files_with("12"));
}

#[test]
fn a_result_for_another_folder_is_refused_and_the_job_stays() {
    let (fs, mut browser) = big();
    typed(&mut browser, "12");
    let other = stale(&browser, browser.view_jobs.next, fs.location("home/docs"));
    assert!(!browser.accept_viewed(other));
    assert_eq!(applied(), 0);
    assert_eq!(names(&browser).len(), ROWS + 3);
    wait_views(&mut browser);
    assert_eq!(names(&browser), files_with("12"));
}

#[test]
fn a_result_whose_rows_are_gone_restores_the_shown_text() {
    let (_fs, mut browser) = big();
    typed(&mut browser, "a");
    let tab = active(&browser);
    browser.full.remove(&tab);
    wait_views(&mut browser);
    assert_eq!(applied(), 0);
    assert_eq!(browser.filter_text(tab), None);
}
