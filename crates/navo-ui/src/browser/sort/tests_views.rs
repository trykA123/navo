use std::sync::Arc;

use navo_core::{AppState, Command, CursorMove, ProviderPath};
use navo_fs::view::{copied_row_bytes, materialize, reset_copied_row_bytes, sort_order};
use navo_fs::{Attributes, CancelToken, EntryKind, FakeFs, FsEntry, ProviderOps};

use super::tests::{names, oracle_order, settle};
use super::{SortBy, SortKey};
use crate::browser::Browser;

const RESERVED: &str = ".navo-mutations-1";

fn entry(name: &str, kind: EntryKind, size: u64, attributes: Attributes) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size,
        modified_ms: Some(i64::try_from(size * 10).unwrap_or(0)),
        attributes,
    }
}

fn hidden() -> Attributes {
    Attributes {
        hidden: true,
        ..Attributes::default()
    }
}

fn system() -> Attributes {
    Attributes {
        system: true,
        ..Attributes::default()
    }
}

fn mixed() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    let plain = Attributes::default;
    let rows = [
        entry("zeta", EntryKind::Folder, 7, plain()),
        entry("b.txt", EntryKind::File, 300, plain()),
        entry(RESERVED, EntryKind::File, 5, plain()),
        entry("alpha", EntryKind::Folder, 3, hidden()),
        entry("a.rs", EntryKind::File, 100, plain()),
        entry("d-hide.txt", EntryKind::File, 50, hidden()),
        entry("C.md", EntryKind::File, 200, plain()),
        entry("e-sys.txt", EntryKind::File, 400, system()),
        entry("x.txt", EntryKind::File, 250, plain()),
    ];
    for row in rows {
        fs.add_entry("home", row);
    }
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.set_show_hidden(true);
    browser.set_show_system(true);
    browser.start();
    settle(&mut browser);
    browser
}

/// Names of `rows(tab)`, checked row for row against the core listing (SL4).
fn aligned(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab().id();
    let rows: Vec<String> = browser.rows(tab).iter().map(|e| e.name.to_text()).collect();
    assert_eq!(rows, names(browser), "rows and listing must stay aligned");
    rows
}

fn typed(browser: &mut Browser, text: &str) {
    for ch in text.chars() {
        assert!(browser.filter_push(ch));
    }
}

fn cursor_to(browser: &mut Browser, name: &str) {
    let row = names(browser).iter().position(|n| n == name).unwrap();
    let to = CursorMove::Row(row as u64);
    browser.command(&Command::PaneCursorMove { to }).unwrap();
}

fn cursor_name(browser: &Browser) -> Option<String> {
    let tab = browser.state().active_tab();
    tab.listing()
        .and_then(|l| l.get(tab.view().cursor))
        .map(|e| e.name.clone())
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

fn size_on_row(browser: &Browser, name: &str) -> Option<u64> {
    let tab = browser.state().active_tab().id();
    let row = names(browser).iter().position(|n| n == name)?;
    browser.rows(tab).get(row).map(|e| e.size)
}

fn every_key() -> Vec<(SortKey, bool)> {
    let bys = [SortBy::Name, SortBy::Ext, SortBy::Size, SortBy::Date];
    bys.into_iter()
        .flat_map(|by| {
            [false, true].into_iter().flat_map(move |descending| {
                [true, false].map(move |folders_first| (SortKey { by, descending }, folders_first))
            })
        })
        .collect()
}

#[test]
fn every_sort_with_and_without_a_filter_equals_the_oracle_and_stays_aligned() {
    let fs = mixed();
    let mut browser = browser(&fs);
    let tab = browser.state().active_tab().id();
    let source = browser.rows(tab).source().clone();
    for text in ["", "txt"] {
        browser.filter_clear();
        if !text.is_empty() {
            typed(&mut browser, text);
        }
        let expected: Vec<FsEntry> = source
            .iter()
            .filter(|e| e.name.to_text() != RESERVED && e.name.to_text().contains(text))
            .cloned()
            .collect();
        for (key, folders_first) in every_key() {
            browser.set_default_sort(key, folders_first);
            let want = oracle_order(&expected, key, folders_first);
            assert_eq!(aligned(&browser), want, "{key:?} {folders_first} {text:?}");
        }
    }
}

#[test]
fn filter_unfilter_and_a_tab_switch_keep_rows_marks_cursor_and_sizes() {
    let fs = mixed();
    let mut browser = browser(&fs);
    let tab = browser.state().active_tab().id();
    assert!(browser.show_size_of(tab, "zeta", 9_999));
    cursor_to(&mut browser, "C.md");
    browser.command(&Command::SelectionToggle).unwrap();
    cursor_to(&mut browser, "a.rs");
    browser.sort_by(SortBy::Size);
    aligned(&browser);
    assert_eq!(cursor_name(&browser).as_deref(), Some("a.rs"));
    assert_eq!(size_on_row(&browser, "zeta"), Some(9_999));
    typed(&mut browser, "txt");
    assert_eq!(
        aligned(&browser),
        ["d-hide.txt", "x.txt", "b.txt", "e-sys.txt"]
    );
    assert_eq!(browser.hidden_marks(tab), 1);
    assert!(browser.filter_clear());
    aligned(&browser);
    assert_eq!(browser.hidden_marks(tab), 0);
    assert_eq!(marked(&browser), ["C.md"]);
    assert_eq!(size_on_row(&browser, "zeta"), Some(9_999));
    cursor_to(&mut browser, "x.txt");
    browser
        .command(&Command::PaneOpenTab { location: None })
        .unwrap();
    settle(&mut browser);
    browser
        .command(&Command::PaneActivateTab { tab: 0 })
        .unwrap();
    settle(&mut browser);
    assert_eq!(browser.state().active_tab().id(), tab);
    aligned(&browser);
    assert_eq!(cursor_name(&browser).as_deref(), Some("x.txt"));
    assert_eq!(marked(&browser), ["C.md"]);
    assert_eq!(size_on_row(&browser, "zeta"), Some(9_999));
}

#[test]
fn a_case_only_tie_keeps_the_folder_order_on_every_resort() {
    let fs = FakeFs::new();
    let plain = Attributes::default;
    fs.add_entry("home", entry("a.txt", EntryKind::File, 2, plain()));
    fs.add_entry("home", entry("A.txt", EntryKind::File, 1, plain()));
    let fs = Arc::new(fs);
    let mut browser = browser(&fs);
    assert_eq!(aligned(&browser), ["a.txt", "A.txt"]);
    browser.sort_by(SortBy::Size);
    assert_eq!(aligned(&browser), ["A.txt", "a.txt"]);
    browser.sort_by(SortBy::Name);
    assert_eq!(aligned(&browser), ["a.txt", "A.txt"]);
}

fn hundred_thousand() -> Arc<[FsEntry]> {
    (0..100_000u64)
        .map(|i| {
            let name = format!("file-{:06}.dat", (i * 7_919) % 100_003);
            entry(
                &name,
                EntryKind::File,
                (i * 31) % 977,
                Attributes::default(),
            )
        })
        .collect()
}

#[expect(
    clippy::print_stderr,
    reason = "the plan's gain is read from the test log"
)]
fn report_copied(before: usize, after: usize) {
    eprintln!("row bytes copied per sort at 100k rows: before {before}, after {after}");
}

#[test]
fn sorting_100k_rows_copies_no_row() {
    let rows = hundred_thousand();
    let key = SortKey {
        by: SortBy::Name,
        descending: false,
    };
    let order = sort_order(&rows, key, true, &CancelToken::new()).unwrap();
    reset_copied_row_bytes();
    let _ = materialize(&rows, &order);
    let before = copied_row_bytes();
    assert!(before >= 3 * 1024 * 1024, "the old copy was at least 3 MiB");
    reset_copied_row_bytes();
    let full = super::sorted_full(&rows, key, true, &super::SortCount::default());
    let after = copied_row_bytes();
    report_copied(before, after);
    assert_eq!(after, 0);
    assert!(Arc::ptr_eq(full.details.source(), &rows));
    let shown: Vec<String> = full.details.iter().map(|e| e.name.to_text()).collect();
    let listed: Vec<&str> = full
        .listing
        .entries()
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(shown, listed);
}
