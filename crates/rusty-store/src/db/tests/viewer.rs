use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::Needle;

use super::*;

const PATH: &str = "file:///home/admin/logs/build.log";
const STAMP: Stamp = Stamp {
    size: 4096,
    modified: 1_760_000_000_000,
};

fn text(text: &str) -> Needle {
    Needle::Text {
        text: text.to_owned(),
        match_case: false,
    }
}

fn memory() -> FileMemory {
    let mut marks = MarkSet::default();
    marks.set(text("error"), 0).unwrap();
    marks.set(Needle::Bytes(vec![0x40, 0x00]), 5).unwrap();
    let mut bookmarks = Bookmarks::default();
    bookmarks.toggle(128);
    bookmarks.toggle(2048);
    FileMemory { marks, bookmarks }
}

fn memory_db() -> Db {
    Db::open_in_memory().unwrap()
}

fn saved() -> Db {
    let db = memory_db();
    db.save_viewer_memory(PATH, STAMP, &memory(), 10).unwrap();
    db
}

fn history(db: &Db) -> Vec<Needle> {
    db.search_history().unwrap().iter().cloned().collect()
}

fn memory_rows(db: &Db) -> i64 {
    db.connection()
        .query_row("SELECT count(*) FROM viewer_memory", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn viewer_memory_round_trips_with_the_same_stamp() {
    let db = saved();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), Some(memory()));
}

#[test]
fn viewer_memory_of_an_unknown_path_is_none() {
    let db = saved();
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
    let other = "file:///home/admin/logs/other.log";
    assert_eq!(db.viewer_memory(other, STAMP).unwrap(), None);
}

#[test]
fn viewer_memory_save_replaces_the_previous_one() {
    let db = saved();
    let mut newer = memory();
    newer.marks.set(text("warning"), 2).unwrap();
    newer.bookmarks.toggle(128);
    db.save_viewer_memory(PATH, STAMP, &newer, 11).unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), Some(newer));
}

#[test]
fn viewer_memory_with_a_new_size_keeps_marks_and_drops_bookmarks() {
    let db = saved();
    let grown = Stamp {
        size: STAMP.size + 1,
        ..STAMP
    };
    let want = FileMemory {
        marks: memory().marks,
        bookmarks: Bookmarks::default(),
    };
    assert_eq!(db.viewer_memory(PATH, grown).unwrap(), Some(want));
}

#[test]
fn viewer_memory_with_a_new_mtime_keeps_marks_and_drops_bookmarks() {
    let db = saved();
    let touched = Stamp {
        modified: STAMP.modified + 1,
        ..STAMP
    };
    let want = FileMemory {
        marks: memory().marks,
        bookmarks: Bookmarks::default(),
    };
    assert_eq!(db.viewer_memory(PATH, touched).unwrap(), Some(want));
}

#[test]
fn saving_an_empty_memory_deletes_the_row() {
    let db = saved();
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
    db.save_viewer_memory(PATH, STAMP, &FileMemory::default(), 11)
        .unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), None);
    assert_eq!(memory_rows(&db), 0);
}

#[test]
fn a_corrupt_viewer_memory_blob_reads_as_none() {
    let db = saved();
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
    let conn = db.connection();
    let mut blobs = conn
        .prepare("SELECT name FROM pragma_table_info('viewer_memory') WHERE upper(type) = 'BLOB'")
        .unwrap();
    let columns: Vec<String> = blobs
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.is_empty(), "viewer_memory has no BLOB column");
    for column in columns {
        let sql = format!("UPDATE viewer_memory SET {column} = x'ffffffffffffffffffffff'");
        conn.execute(&sql, []).unwrap();
    }
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), None);
}

#[test]
fn a_corrupt_bookmarks_blob_alone_reads_as_none() {
    let db = saved();
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
    db.connection()
        .execute(
            "UPDATE viewer_memory SET bookmarks = x'ffffffffffffffffffffff'",
            [],
        )
        .unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), None);
}

#[test]
fn marks_that_decode_but_break_the_rules_read_as_none() {
    let db = saved();
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
    db.connection()
        .execute("UPDATE viewer_memory SET marks = x'0100000009'", [])
        .unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), None);
}

#[test]
fn viewer_memory_survives_reopening_the_file() {
    let dir = tmp();
    let path = db_path(&dir);
    let (db, _) = Db::open(&path, false).unwrap();
    db.save_viewer_memory(PATH, STAMP, &memory(), 10).unwrap();
    db.remember_search(&text("error"), 10).unwrap();
    drop(db);
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), Some(memory()));
    assert_eq!(history(&db), vec![text("error")]);
}

#[test]
fn a_migrated_v1_db_remembers_viewer_state() {
    let dir = tmp();
    let path = db_path(&dir);
    build_from_sql(&path, V1_SQL);
    let (db, _) = Db::open(&path, false).unwrap();
    db.save_viewer_memory(PATH, STAMP, &memory(), 10).unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), Some(memory()));
}

#[test]
fn search_history_lists_newest_first() {
    let db = memory_db();
    db.remember_search(&text("alpha"), 1).unwrap();
    db.remember_search(&text("beta"), 2).unwrap();
    db.remember_search(&text("gamma"), 3).unwrap();
    assert_eq!(
        history(&db),
        vec![text("gamma"), text("beta"), text("alpha")]
    );
}

#[test]
fn search_history_moves_a_repeat_to_the_front() {
    let db = memory_db();
    db.remember_search(&text("alpha"), 1).unwrap();
    db.remember_search(&text("beta"), 2).unwrap();
    db.remember_search(&text("alpha"), 3).unwrap();
    assert_eq!(history(&db), vec![text("alpha"), text("beta")]);
}

#[test]
fn search_history_keeps_text_and_hex_apart() {
    let db = memory_db();
    let hex = Needle::Bytes(b"AB".to_vec());
    db.remember_search(&text("AB"), 1).unwrap();
    db.remember_search(&hex, 2).unwrap();
    assert_eq!(history(&db), vec![hex, text("AB")]);
}

#[test]
fn search_history_keeps_at_most_the_cap() {
    let db = memory_db();
    let extra = 5;
    for index in 0..SearchHistory::MAX + extra {
        let now = i64::try_from(index).unwrap();
        db.remember_search(&text(&format!("word{index}")), now)
            .unwrap();
    }
    let got = history(&db);
    assert_eq!(got.len(), SearchHistory::MAX);
    assert_eq!(got.first(), Some(&text("word54")));
    assert_eq!(got.last(), Some(&text("word5")));
    let rows: i64 = db
        .connection()
        .query_row("SELECT count(*) FROM search_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, SearchHistory::MAX as i64);
}

#[test]
fn prune_keeps_the_most_recently_saved_files() {
    let db = memory_db();
    let paths = ["file:///a.log", "file:///b.log", "file:///c.log"];
    for (path, now) in paths.iter().zip([1, 3, 2]) {
        db.save_viewer_memory(path, STAMP, &memory(), now).unwrap();
    }
    assert_eq!(db.prune_viewer_memory(2).unwrap(), 1);
    let kept: Vec<bool> = paths
        .iter()
        .map(|path| db.viewer_memory(path, STAMP).unwrap().is_some())
        .collect();
    assert_eq!(kept, vec![false, true, true]);
}

#[test]
fn prune_with_room_to_spare_removes_nothing() {
    let db = saved();
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
    assert_eq!(db.prune_viewer_memory(5).unwrap(), 0);
    assert!(db.viewer_memory(PATH, STAMP).unwrap().is_some());
}

#[test]
fn a_fresh_in_memory_db_saves_and_reads_viewer_state() {
    let db = memory_db();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), None);
    assert!(history(&db).is_empty());
    db.save_viewer_memory(PATH, STAMP, &memory(), 10).unwrap();
    db.remember_search(&text("error"), 10).unwrap();
    assert_eq!(db.viewer_memory(PATH, STAMP).unwrap(), Some(memory()));
    assert_eq!(history(&db), vec![text("error")]);
}
