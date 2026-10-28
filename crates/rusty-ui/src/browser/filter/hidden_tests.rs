use std::sync::Arc;

use rusty_core::{AppState, Command, CursorMove, Direction, ProviderPath, apply};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry, ProviderOps};

use super::tests::{names, settle};
use crate::browser::Browser;

fn entry(name: &str, attributes: Attributes) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 1,
        modified_ms: None,
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

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/a.txt", 1);
    fs.add_entry("home", entry("b-secret.txt", hidden()));
    fs.add_entry("home", entry("c-boot.ini", system()));
    fs.add_file("home/d.txt", 1);
    fs.add_folder("other");
    fs.add_entry("other", entry("only-hidden", hidden()));
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut state = AppState::new(fs.location("home"));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    settle(&mut browser);
    browser
}

fn cursor_name(browser: &Browser) -> Option<String> {
    let tab = browser.state().active_tab();
    let cursor = tab.view().cursor;
    tab.listing()?.get(cursor).map(|e| e.name.clone())
}

fn every_pane_names(browser: &Browser) -> Vec<Vec<String>> {
    let state = browser.state();
    state
        .window(state.focused_window())
        .map(|window| {
            window
                .panes()
                .iter()
                .map(|pane| {
                    let tab = pane.active();
                    tab.listing()
                        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn hidden_files_show_by_default() {
    let fs = fake();
    let browser = browser(&fs);
    assert!(browser.shows_hidden());
    assert_eq!(
        names(&browser),
        ["docs", "a.txt", "b-secret.txt", "c-boot.ini", "d.txt"]
    );
}

#[test]
fn turning_hidden_off_drops_hidden_and_system_entries_in_both_panes() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.set_show_hidden(false);
    let both = every_pane_names(&browser);
    assert_eq!(both.len(), 2);
    for names in both {
        assert_eq!(names, ["docs", "a.txt", "d.txt"]);
    }
    let tab = browser.state().active_tab().id();
    let details: Vec<String> = browser
        .details(tab)
        .iter()
        .map(|e| e.name.to_text())
        .collect();
    assert_eq!(details, ["docs", "a.txt", "d.txt"]);
}

#[test]
fn turning_hidden_back_on_restores_every_entry() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.set_show_hidden(false);
    browser.set_show_hidden(true);
    assert_eq!(
        names(&browser),
        ["docs", "a.txt", "b-secret.txt", "c-boot.ini", "d.txt"]
    );
}

#[test]
fn the_cursor_stays_on_its_name_when_the_rows_change() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Row(4),
        })
        .expect("cursor");
    assert_eq!(cursor_name(&browser).as_deref(), Some("d.txt"));
    browser.set_show_hidden(false);
    assert_eq!(cursor_name(&browser).as_deref(), Some("d.txt"));
}

#[test]
fn a_folder_opened_while_hidden_is_off_stays_filtered() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.set_show_hidden(false);
    browser
        .command(&Command::PaneNavigate {
            location: fs.location("other"),
        })
        .expect("navigate");
    settle(&mut browser);
    assert!(names(&browser).is_empty());
}

#[test]
fn type_to_filter_works_on_the_visible_rows_only() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.set_show_hidden(false);
    assert!(!browser.filter_push('b'));
    assert!(browser.filter_push('d'));
    assert_eq!(names(&browser), ["docs", "d.txt"]);
    browser.set_show_hidden(true);
    assert_eq!(names(&browser), ["docs", "d.txt"]);
}

#[test]
fn hiding_the_cursor_entry_moves_to_the_next_visible_entry_not_the_top() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Row(2),
        })
        .expect("cursor");
    assert_eq!(cursor_name(&browser).as_deref(), Some("b-secret.txt"));
    browser.set_show_hidden(false);
    assert_eq!(cursor_name(&browser).as_deref(), Some("d.txt"));
}

#[test]
fn hiding_the_last_entry_moves_back_to_the_previous_visible_entry() {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/a.txt", 1);
    fs.add_entry("home", entry("z-hidden.txt", hidden()));
    let fs = Arc::new(fs);
    let mut browser = browser(&fs);
    browser
        .command(&Command::PaneCursorMove {
            to: CursorMove::Row(2),
        })
        .expect("cursor");
    browser.set_show_hidden(false);
    assert_eq!(cursor_name(&browser).as_deref(), Some("a.txt"));
}

#[test]
fn system_files_hide_when_system_is_off_even_with_hidden_shown() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.set_show_system(false);
    assert!(!browser.shows_system());
    assert_eq!(names(&browser), ["docs", "a.txt", "b-secret.txt", "d.txt"]);
    browser.set_show_system(true);
    assert_eq!(
        names(&browser),
        ["docs", "a.txt", "b-secret.txt", "c-boot.ini", "d.txt"]
    );
}

#[test]
fn reserved_entries_never_show_even_with_hidden_on() {
    let fs = FakeFs::new();
    for name in [".navo-retained-abc", ".NAVO-HELD-x", ".ferrite-retained-y"] {
        fs.add_entry("home", entry(name, Attributes::default()));
    }
    fs.add_entry("home", entry(".config", hidden()));
    let browser = browser(&Arc::new(fs));
    assert!(browser.shows_hidden());
    assert_eq!(names(&browser), [".config"]);
}
