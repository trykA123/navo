use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{AppState, Command, CursorMove, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry, ProviderOps};

use super::{SortBy, SortKey};
use crate::browser::Browser;

fn file(name: &str, size: u64, modified_ms: i64) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size,
        modified_ms: Some(modified_ms),
        attributes: Attributes::default(),
    }
}

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/zeta");
    fs.add_folder("home/alpha");
    fs.add_entry("home", file("b.txt", 300, 3_000));
    fs.add_entry("home", file("a.rs", 100, 1_000));
    fs.add_entry("home", file("C.md", 200, 2_000));
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.start();
    settle(&mut browser);
    browser
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

fn detail_names(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab().id();
    browser
        .details(tab)
        .iter()
        .map(|e| e.name.to_text())
        .collect()
}

#[test]
fn names_sort_folders_first_then_without_case() {
    let fs = fake();
    let browser = browser(&fs);
    assert_eq!(names(&browser), ["alpha", "zeta", "a.rs", "b.txt", "C.md"]);
}

#[test]
fn size_extension_and_date_sort_files_and_keep_folders_first() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.sort_by(SortBy::Size);
    assert_eq!(names(&browser), ["alpha", "zeta", "a.rs", "C.md", "b.txt"]);
    browser.sort_by(SortBy::Ext);
    assert_eq!(names(&browser), ["alpha", "zeta", "C.md", "a.rs", "b.txt"]);
    browser.sort_by(SortBy::Date);
    assert_eq!(names(&browser), ["alpha", "zeta", "a.rs", "C.md", "b.txt"]);
    assert_eq!(names(&browser), detail_names(&browser));
}

#[test]
fn the_same_column_twice_reverses_but_folders_stay_first() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.sort_by(SortBy::Name);
    let tab = browser.state().active_tab().id();
    assert_eq!(
        browser.sort_key(tab),
        SortKey {
            by: SortBy::Name,
            descending: true
        }
    );
    assert_eq!(names(&browser), ["zeta", "alpha", "C.md", "b.txt", "a.rs"]);
}

#[test]
fn the_cursor_follows_its_entry_through_a_sort() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Row(4),
        })
        .unwrap();
    browser.sort_by(SortBy::Size);
    let tab = browser.state().active_tab();
    assert_eq!(tab.view().cursor, 3);
}

#[test]
fn a_refresh_keeps_the_sort_and_the_filter() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.sort_by(SortBy::Size);
    browser.filter_push('*');
    browser.filter_push('t');
    browser.command(&Command::PaneRefresh).unwrap();
    settle(&mut browser);
    assert_eq!(names(&browser), ["zeta", "b.txt"]);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.sort_key(tab).by, SortBy::Size);
}

fn marked_names(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab();
    let listing = tab
        .listing()
        .cloned()
        .unwrap_or_else(|| rusty_core::Listing::new(Vec::new()));
    tab.selection()
        .marked()
        .into_iter()
        .filter_map(|row| listing.get(row).map(|e| e.name.clone()))
        .collect()
}

#[test]
fn marks_follow_their_names_through_a_sort() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Row(3),
        })
        .unwrap();
    browser.command(&Command::SelectionToggle).unwrap();
    browser.sort_by(SortBy::Size);
    assert_eq!(marked_names(&browser), ["b.txt"]);
}

#[test]
fn sorting_under_an_active_filter_keeps_rows_and_details_aligned() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.filter_push('*');
    browser.filter_push('t');
    browser.sort_by(SortBy::Size);
    browser.sort_by(SortBy::Size);
    assert_eq!(names(&browser), ["zeta", "b.txt"]);
    assert_eq!(names(&browser), detail_names(&browser));
}

#[test]
fn a_folder_measured_after_a_sort_gets_its_own_size() {
    let fs = fake();
    fs.add_file("home/zeta/big.bin", 9_000);
    let mut browser = browser(&fs);
    browser.sort_by(SortBy::Name);
    assert_eq!(names(&browser)[0], "zeta");
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Row(0),
        })
        .unwrap();
    browser.command(&Command::SelectionToggle).unwrap();
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    let zeta = browser
        .details(tab)
        .iter()
        .find(|e| e.name.to_text() == "zeta")
        .map(|e| e.size);
    assert_eq!(zeta, Some(9_000));
}
