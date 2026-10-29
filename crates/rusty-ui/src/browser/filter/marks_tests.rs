use std::sync::Arc;

use rusty_core::{AppState, Command, ProviderPath};
use rusty_fs::{FakeFs, ProviderOps};

use super::tests::{names, settle};
use crate::browser::Browser;

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/alpha.txt", 1);
    fs.add_file("home/apple.txt", 1);
    fs.add_file("home/beta.txt", 1);
    fs.add_file("home/bravo.txt", 1);
    fs.add_file("home/cargo.txt", 1);
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let state = AppState::new(fs.location("home"));
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    settle(&mut browser);
    browser
}

fn typed(browser: &mut Browser, text: &str) {
    for ch in text.chars() {
        browser.filter_push(ch);
    }
}

fn mark_all(browser: &mut Browser) {
    browser.command(&Command::SelectionAll).unwrap();
}

fn marked_names(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab();
    let listing = tab.listing().unwrap();
    tab.selection()
        .marked()
        .into_iter()
        .filter_map(|row| listing.get(row).map(|e| e.name.clone()))
        .collect()
}

fn sources(browser: &Browser) -> Vec<String> {
    browser
        .selected_sources()
        .iter()
        .map(|l| l.path().to_text())
        .map(|p| p.rsplit('/').next().unwrap_or_default().to_owned())
        .collect()
}

fn active(browser: &Browser) -> rusty_core::TabId {
    browser.state().active_tab().id()
}

fn two_groups(fs: &Arc<FakeFs>) -> Browser {
    let mut browser = browser(fs);
    typed(&mut browser, "^a");
    mark_all(&mut browser);
    browser.filter_clear();
    typed(&mut browser, "^b");
    mark_all(&mut browser);
    browser
}

#[test]
fn marks_on_rows_hidden_by_the_filter_come_back_when_it_clears() {
    let fs = fake();
    let mut browser = two_groups(&fs);
    assert_eq!(marked_names(&browser), ["beta.txt", "bravo.txt"]);
    assert_eq!(browser.hidden_marks(active(&browser)), 2);
    browser.filter_clear();
    assert_eq!(
        marked_names(&browser),
        ["alpha.txt", "apple.txt", "beta.txt", "bravo.txt"]
    );
    assert_eq!(browser.hidden_marks(active(&browser)), 0);
}

#[test]
fn marks_come_back_when_the_filter_changes_to_show_them() {
    let fs = fake();
    let mut browser = two_groups(&fs);
    browser.filter_clear();
    typed(&mut browser, "^a");
    assert_eq!(names(&browser), ["alpha.txt", "apple.txt"]);
    assert_eq!(marked_names(&browser), ["alpha.txt", "apple.txt"]);
    assert_eq!(browser.hidden_marks(active(&browser)), 2);
}

#[test]
fn selected_sources_never_contains_hidden_rows() {
    let fs = fake();
    let browser = two_groups(&fs);
    assert_eq!(sources(&browser), ["beta.txt", "bravo.txt"]);
}

#[test]
fn marks_hidden_by_a_longer_filter_stay_out_of_the_sources() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "^a");
    mark_all(&mut browser);
    typed(&mut browser, "l");
    assert_eq!(browser.hidden_marks(active(&browser)), 1);
    assert_eq!(sources(&browser), ["alpha.txt"]);
}

#[test]
fn navigation_empties_the_hidden_marks() {
    let fs = fake();
    let mut browser = two_groups(&fs);
    let location = browser
        .state()
        .active_tab()
        .location()
        .join(&ProviderPath::from_text("docs"))
        .unwrap();
    browser
        .command(&Command::PaneNavigate { location })
        .unwrap();
    settle(&mut browser);
    assert_eq!(browser.hidden_marks(active(&browser)), 0);
    browser.command(&Command::PaneBack).unwrap();
    settle(&mut browser);
    assert_eq!(browser.hidden_marks(active(&browser)), 0);
    assert_eq!(marked_names(&browser), Vec::<String>::new());
}

#[test]
fn unmark_all_clears_the_hidden_marks_too() {
    let fs = fake();
    let mut browser = two_groups(&fs);
    browser.command(&Command::SelectionClear).unwrap();
    assert_eq!(browser.hidden_marks(active(&browser)), 0);
    browser.filter_clear();
    assert_eq!(marked_names(&browser), Vec::<String>::new());
}

#[test]
fn hiding_every_mark_counts_them_and_clearing_the_filter_resets_the_count() {
    let fs = fake();
    let mut browser = browser(&fs);
    mark_all(&mut browser);
    typed(&mut browser, "^a");
    assert_eq!(browser.hidden_marks(active(&browser)), 4);
    browser.filter_clear();
    assert_eq!(browser.hidden_marks(active(&browser)), 0);
    assert_eq!(marked_names(&browser).len(), 6);
}

#[test]
fn unmark_all_saves_the_hidden_marks_so_restore_brings_all_back() {
    let fs = fake();
    let mut browser = two_groups(&fs);
    browser.command(&Command::SelectionClear).unwrap();
    browser.filter_clear();
    browser.command(&Command::SelectionRestore).unwrap();
    assert_eq!(
        marked_names(&browser),
        ["alpha.txt", "apple.txt", "beta.txt", "bravo.txt"]
    );
}

#[test]
fn unmark_all_with_only_hidden_marks_saves_them() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "^a");
    mark_all(&mut browser);
    browser.filter_clear();
    typed(&mut browser, "^b");
    browser.command(&Command::SelectionClear).unwrap();
    browser.filter_clear();
    browser.command(&Command::SelectionRestore).unwrap();
    assert_eq!(marked_names(&browser), ["alpha.txt", "apple.txt"]);
}

#[test]
fn restore_under_a_filter_keeps_the_filtered_names_for_later() {
    let fs = fake();
    let mut browser = browser(&fs);
    mark_all(&mut browser);
    browser.command(&Command::SelectionClear).unwrap();
    typed(&mut browser, "^a");
    browser.command(&Command::SelectionRestore).unwrap();
    assert_eq!(marked_names(&browser), ["alpha.txt", "apple.txt"]);
    assert_eq!(browser.hidden_marks(active(&browser)), 4);
    browser.filter_clear();
    assert_eq!(marked_names(&browser).len(), 6);
}

#[test]
fn restore_succeeds_when_the_filter_hides_every_saved_name() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "^b");
    mark_all(&mut browser);
    browser.command(&Command::SelectionClear).unwrap();
    browser.filter_clear();
    typed(&mut browser, "^a");
    browser.command(&Command::SelectionRestore).unwrap();
    assert_eq!(browser.hidden_marks(active(&browser)), 2);
    browser.filter_clear();
    assert_eq!(marked_names(&browser), ["beta.txt", "bravo.txt"]);
}
