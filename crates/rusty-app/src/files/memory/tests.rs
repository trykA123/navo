use std::path::Path;
use std::sync::PoisonError;

use rusty_core::Location;
use rusty_core::marks::{Bookmarks, MarkSet};
use rusty_core::paging::Needle;
use rusty_ui::window_view::{FileHooks, FileStamp};

use super::{attach, open, open_keeping};

const STAMP: FileStamp = FileStamp {
    len: 120,
    modified_ms: 1_700_000_000_000,
};

fn needle(text: &str) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case: false,
    }
}

fn hooks_on(file: &Path) -> FileHooks {
    attach(FileHooks::none(), open(file).unwrap())
}

fn location() -> Location {
    Location::new("/logs/app.log").unwrap()
}

fn remembered() -> (MarkSet, Bookmarks) {
    let mut marks = MarkSet::default();
    marks.set(needle("error"), 2).unwrap();
    let mut bookmarks = Bookmarks::default();
    bookmarks.toggle(40);
    (marks, bookmarks)
}

#[test]
fn marks_and_bookmarks_come_back_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let (marks, bookmarks) = remembered();
    (hooks_on(&file).save_memory)(&location(), STAMP, &marks, &bookmarks);
    let loaded = (hooks_on(&file).memory)(&location(), STAMP);
    assert_eq!(loaded, Some((marks, bookmarks)));
}

#[test]
fn a_changed_file_keeps_its_marks_but_loses_its_bookmarks() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let (marks, bookmarks) = remembered();
    (hooks_on(&file).save_memory)(&location(), STAMP, &marks, &bookmarks);
    let changed = FileStamp {
        len: STAMP.len + 1,
        ..STAMP
    };
    let loaded = (hooks_on(&file).memory)(&location(), changed);
    assert_eq!(loaded, Some((marks, Bookmarks::default())));
}

#[test]
fn search_history_persists_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let hooks = hooks_on(&file);
    (hooks.remember)(&needle("first"));
    (hooks.remember)(&needle("second"));
    drop(hooks);
    let history = (hooks_on(&file).history)();
    let recalled: Vec<&Needle> = history.iter().collect();
    assert_eq!(recalled, [&needle("second"), &needle("first")]);
}

#[test]
fn a_failing_db_yields_no_memory_and_no_panic() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir.path().join("ferrite.db")).unwrap();
    db.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .connection()
        .execute_batch("DROP TABLE viewer_memory; DROP TABLE search_history;")
        .unwrap();
    let hooks = attach(FileHooks::none(), db);
    let (marks, bookmarks) = remembered();
    (hooks.save_memory)(&location(), STAMP, &marks, &bookmarks);
    (hooks.remember)(&needle("lost"));
    assert_eq!((hooks.memory)(&location(), STAMP), None);
    assert_eq!((hooks.history)().iter().count(), 0);
}

#[test]
fn a_db_that_cannot_open_gives_no_handle() {
    let dir = tempfile::tempdir().unwrap();
    assert!(open(dir.path()).is_none());
}

#[test]
fn opening_keeps_only_the_newest_file_memories() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let (marks, bookmarks) = remembered();
    let paths = ["/logs/a.log", "/logs/b.log", "/logs/c.log"];
    for path in paths {
        let hooks = hooks_on(&file);
        (hooks.save_memory)(&Location::new(path).unwrap(), STAMP, &marks, &bookmarks);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let hooks = attach(FileHooks::none(), open_keeping(&file, 2).unwrap());
    let kept = |path: &str| (hooks.memory)(&Location::new(path).unwrap(), STAMP).is_some();
    assert!(!kept(paths[0]));
    assert!(kept(paths[1]) && kept(paths[2]));
}

#[test]
fn the_markdown_split_choice_comes_back_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    assert_eq!((hooks_on(&file).split)("md"), None);
    (hooks_on(&file).save_split)("md", false);
    assert_eq!((hooks_on(&file).split)("md"), Some(false));
    assert_eq!((hooks_on(&file).split)("markdown"), None);
}
