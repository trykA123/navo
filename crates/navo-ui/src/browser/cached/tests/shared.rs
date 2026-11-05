use std::sync::Arc;

use navo_core::{Command, TabId};
use navo_fs::view::Rows;
use navo_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::{big_fake, cursor_name, names, put_cursor_on, settle, started};
use crate::browser::Browser;
use crate::file_list::{SortBy, SortKey};

const BY_SIZE: SortKey = SortKey {
    by: SortBy::Size,
    descending: false,
};

/// Names grow, sizes shrink and dates go back and forth, so every key orders differently.
fn spread() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for (index, name) in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]
        .iter()
        .enumerate()
    {
        let row = i64::try_from(index).unwrap_or(0);
        fs.add_entry(
            "home",
            FsEntry {
                name: navo_core::ProviderPath::from_text(name),
                kind: EntryKind::File,
                size: 10 - index as u64,
                modified_ms: Some((row * 7) % 5),
                attributes: Attributes::default(),
            },
        );
    }
    Arc::new(fs)
}

fn by_size(fs: &Arc<FakeFs>) -> Browser {
    let mut browser = started(fs);
    browser.set_default_sort(BY_SIZE, true);
    browser.start();
    settle(&mut browser);
    browser
}

fn open_second_tab(browser: &mut Browser) -> (TabId, TabId) {
    let first = browser.state().active_tab().id();
    browser
        .command(&Command::PaneOpenTab { location: None })
        .unwrap();
    settle(browser);
    (first, browser.state().active_tab().id())
}

fn activate(browser: &mut Browser, tab: u32) {
    browser.command(&Command::PaneActivateTab { tab }).unwrap();
    settle(browser);
}

/// Names of `rows(tab)`, checked row for row against the tab's listing (SL4).
fn aligned(browser: &Browser, tab: TabId) -> Vec<String> {
    let rows: Vec<String> = browser.rows(tab).iter().map(|e| e.name.to_text()).collect();
    let listing: Vec<String> = browser
        .state()
        .tab(tab)
        .and_then(|t| t.listing())
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default();
    assert_eq!(rows, listing, "rows and listing must stay aligned");
    rows
}

fn marked(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab();
    let listing = tab.listing().unwrap();
    tab.selection()
        .marked()
        .into_iter()
        .filter_map(|row| listing.get(row).map(|e| e.name.clone()))
        .collect()
}

#[test]
fn two_tabs_on_one_folder_with_one_key_share_one_sort() {
    let fs = spread();
    let mut browser = by_size(&fs);
    assert_eq!(browser.sorts(), 1);
    let (first, second) = open_second_tab(&mut browser);
    assert_eq!(browser.sorts(), 1);
    assert!(Rows::same(&browser.rows(first), &browser.rows(second)));
    let want = ["e.txt", "d.txt", "c.txt", "b.txt", "a.txt"];
    assert_eq!(aligned(&browser, first), want);
    assert_eq!(aligned(&browser, second), want);
}

#[test]
fn two_tabs_on_one_folder_with_two_keys_sort_twice() {
    let fs = spread();
    let mut browser = by_size(&fs);
    let (first, second) = open_second_tab(&mut browser);
    browser.sort_by(SortBy::Date);
    assert_eq!(browser.sorts(), 2);
    assert!(!Rows::same(&browser.rows(first), &browser.rows(second)));
    assert_eq!(
        aligned(&browser, first),
        ["e.txt", "d.txt", "c.txt", "b.txt", "a.txt"]
    );
    assert_eq!(
        aligned(&browser, second),
        ["a.txt", "d.txt", "b.txt", "e.txt", "c.txt"]
    );
}

#[test]
fn a_user_sort_on_two_tabs_of_one_folder_runs_once() {
    let fs = spread();
    let mut browser = by_size(&fs);
    let (first, second) = open_second_tab(&mut browser);
    let sorts = browser.sorts();
    activate(&mut browser, 0);
    assert!(browser.sort_by(SortBy::Date));
    assert_eq!(browser.sorts(), sorts + 1);
    activate(&mut browser, 1);
    assert!(browser.sort_by(SortBy::Date));
    assert_eq!(browser.sorts(), sorts + 1);
    assert!(Rows::same(&browser.rows(first), &browser.rows(second)));
    let want = ["a.txt", "d.txt", "b.txt", "e.txt", "c.txt"];
    assert_eq!(aligned(&browser, first), want);
    assert_eq!(aligned(&browser, second), want);
}

#[test]
fn a_user_sort_with_measured_sizes_sorts_the_tabs_own_copy_and_leaves_the_other_tab_alone() {
    let fs = spread();
    let mut browser = by_size(&fs);
    let (first, second) = open_second_tab(&mut browser);
    let before = browser.rows(second);
    activate(&mut browser, 0);
    browser.remember_sizes(first, &[("a.txt".to_owned(), 99)]);
    let sorts = browser.sorts();
    assert!(browser.sort_by(SortBy::Date));
    assert_eq!(browser.sorts(), sorts + 1);
    let own = browser.rows(first);
    assert!(!Arc::ptr_eq(own.source(), before.source()));
    assert!(
        own.iter()
            .any(|e| e.name.to_text() == "a.txt" && e.size == 99)
    );
    assert!(Rows::same(&before, &browser.rows(second)));
    assert_eq!(
        aligned(&browser, first),
        ["a.txt", "d.txt", "b.txt", "e.txt", "c.txt"]
    );
    assert_eq!(
        aligned(&browser, second),
        ["e.txt", "d.txt", "c.txt", "b.txt", "a.txt"]
    );
}

#[test]
fn a_filtered_tab_narrows_the_shared_sort_and_each_tab_keeps_its_marks_and_cursor() {
    let fs = spread();
    let mut browser = by_size(&fs);
    put_cursor_on(&mut browser, "b.txt");
    browser.command(&Command::SelectionToggle).unwrap();
    put_cursor_on(&mut browser, "c.txt");
    let (first, second) = open_second_tab(&mut browser);
    put_cursor_on(&mut browser, "d.txt");
    browser.command(&Command::SelectionToggle).unwrap();
    for ch in "d.".chars() {
        assert!(browser.filter_push(ch));
    }
    assert_eq!(browser.sorts(), 1);
    assert!(Arc::ptr_eq(
        browser.rows(first).source(),
        browser.rows(second).source()
    ));
    assert_eq!(aligned(&browser, second), ["d.txt"]);
    assert_eq!(cursor_name(&browser).as_deref(), Some("d.txt"));
    assert_eq!(marked(&browser), ["d.txt"]);
    activate(&mut browser, 0);
    assert_eq!(
        aligned(&browser, first),
        ["e.txt", "d.txt", "c.txt", "b.txt", "a.txt"]
    );
    assert_eq!(cursor_name(&browser).as_deref(), Some("c.txt"));
    assert_eq!(marked(&browser), ["b.txt"]);
    activate(&mut browser, 1);
    assert_eq!(aligned(&browser, second), ["d.txt"]);
    assert_eq!(marked(&browser), ["d.txt"]);
    assert_eq!(browser.sorts(), 1);
}

#[test]
fn a_second_tab_on_a_big_folder_shows_the_shared_sort_without_an_early_job() {
    let fs = big_fake(30_000);
    let mut browser = by_size(&fs);
    let sorts = browser.sorts();
    let first = browser.state().active_tab().id();
    browser
        .command(&Command::PaneOpenTab { location: None })
        .unwrap();
    let second = browser.state().active_tab().id();
    assert!(browser.is_cached(second), "shown at once from the memo");
    assert!(Rows::same(&browser.rows(first), &browser.rows(second)));
    settle(&mut browser);
    assert_eq!(browser.sorts(), sorts);
    assert_eq!(
        names(&browser).first().map(String::as_str),
        Some("f29999.txt")
    );
    aligned(&browser, second);
}

#[test]
fn closing_a_sharing_tab_drops_its_memo_and_keeps_the_other_view() {
    let fs = spread();
    let mut browser = by_size(&fs);
    let (first, second) = open_second_tab(&mut browser);
    let kept = browser.rows(first);
    browser
        .command(&Command::PaneCloseTab { tab: None })
        .unwrap();
    assert!(!browser.sorted.contains_key(&second));
    assert!(browser.sorted.contains_key(&first));
    assert!(Rows::same(&kept, &browser.rows(first)));
    assert_eq!(
        aligned(&browser, first),
        ["e.txt", "d.txt", "c.txt", "b.txt", "a.txt"]
    );
}
