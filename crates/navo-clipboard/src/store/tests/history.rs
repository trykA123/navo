use std::fs;
use std::path::PathBuf;

use rusqlite::Connection;
use tempfile::TempDir;

use super::{entry, path_entry, temp_store, text};
use crate::limits::Limits;
use crate::model::{Content, Transfer};
use crate::store::{ClipError, ClipStore, Stored};

fn texts(rows: &[Stored]) -> Vec<&str> {
    rows.iter()
        .filter_map(|row| match &row.entry.content {
            Content::Text(text) => Some(text.as_str()),
            Content::Paths { .. } => None,
        })
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn five_hundred_and_one_records_keep_the_newest_five_hundred() {
    let (_dir, mut store) = temp_store(Limits::default());
    for at in 1..=501 {
        store.record(&text(&format!("body-{at}"), at)).unwrap();
    }
    let rows = store.history(usize::MAX).unwrap();
    assert_eq!(rows.len(), 500);
    assert_eq!(rows.first().map(|row| row.entry.last_ms), Some(501));
    assert_eq!(rows.last().map(|row| row.entry.last_ms), Some(2));
    assert!(!rows.iter().any(|row| row.entry.last_ms == 1));
}

#[test]
fn a_repeat_moves_to_the_top_and_counts_uses() {
    let (_dir, mut store) = temp_store(Limits::default());
    store.record(&text("same", 10)).unwrap();
    let first = store.history(10).unwrap();
    assert_eq!(first.len(), 1);
    let id = first[0].id;
    let mut again = text("same", 20);
    again.source_app = Some("other.exe".into());
    store.record(&again).unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].entry.first_ms, 10);
    assert_eq!(rows[0].entry.last_ms, 20);
    assert_eq!(rows[0].entry.uses, 2);
    assert_eq!(rows[0].entry.source_app.as_deref(), Some("other.exe"));
}

#[test]
fn a_reopened_store_lists_the_same_rows() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("clipboard.db");
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    store.record(&text("first", 1)).unwrap();
    store
        .record(&path_entry(
            &["C:/one/two", "C:\\three\\four"],
            Transfer::Cut,
            2,
        ))
        .unwrap();
    let before = store.history(10).unwrap();
    drop(store);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.history(10).unwrap(), before);
}

#[test]
fn a_newer_database_version_is_refused() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("clipboard.db");
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    let result = ClipStore::open(&path, Limits::default());
    assert!(matches!(result, Err(ClipError::TooNew { found: 99, .. })));
}

#[test]
fn the_text_byte_cap_evicts_the_oldest_rows() {
    let limits = Limits {
        max_history_text_bytes: 10,
        ..Limits::default()
    };
    let (_dir, mut store) = temp_store(limits);
    store.record(&text("aaaa", 1)).unwrap();
    store.record(&text("bbbb", 2)).unwrap();
    store.record(&text("cccc", 3)).unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(texts(&rows), ["cccc", "bbbb"]);
    store.record(&text("ddddd", 4)).unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(texts(&rows), ["ddddd", "cccc"]);
}

#[test]
fn delete_removes_one_row() {
    let (_dir, mut store) = temp_store(Limits::default());
    store.record(&text("first", 1)).unwrap();
    store.record(&text("second", 2)).unwrap();
    let rows = store.history(10).unwrap();
    let oldest = rows.last().unwrap().id;
    store.delete(oldest).unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(texts(&rows), ["second"]);
}

#[test]
fn clear_history_empties_the_table_and_the_wal() {
    let (dir, mut store) = temp_store(Limits::default());
    let canary = "canary-clear-history-body";
    store.record(&text(canary, 1)).unwrap();
    store.clear_history().unwrap();
    assert!(store.history(10).unwrap().is_empty());
    for entry in fs::read_dir(dir.path()).unwrap() {
        let path = entry.unwrap().path();
        let bytes = fs::read(&path).unwrap();
        assert!(
            !contains(&bytes, canary.as_bytes()),
            "canary left in a database file"
        );
    }
}

#[test]
fn path_rows_round_trip_with_either_separator() {
    let (_dir, mut store) = temp_store(Limits::default());
    let paths = vec![
        PathBuf::from("C:/one/two"),
        PathBuf::from("C:\\three\\four"),
        PathBuf::from("C:/ünïcode"),
    ];
    let content = Content::Paths {
        paths: paths.clone(),
        transfer: Transfer::Cut,
    };
    store.record(&entry(content.clone(), 1)).unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.content, content);
}

#[test]
fn a_record_with_an_old_clock_time_survives_the_trim_and_lands_on_top() {
    let (_dir, mut store) = temp_store(Limits::default());
    for at in 1..=500 {
        store
            .record(&text(&format!("body-{at}"), 1000 + at))
            .unwrap();
    }
    store.record(&text("stepped-back", 0)).unwrap();
    let rows = store.history(usize::MAX).unwrap();
    assert_eq!(rows.len(), 500);
    assert_eq!(texts(&rows).first(), Some(&"stepped-back"));
    assert!(!texts(&rows).contains(&"body-1"));
}

#[test]
fn a_repeat_with_an_older_time_never_moves_down() {
    let (_dir, mut store) = temp_store(Limits::default());
    store.record(&text("same", 50)).unwrap();
    store.record(&text("same", 10)).unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].entry.last_ms, 50);
    assert_eq!(rows[0].entry.uses, 2);
}

#[test]
fn a_repeat_of_the_lower_row_moves_above_the_other_row() {
    let (_dir, mut store) = temp_store(Limits::default());
    store.record(&text("lower", 10)).unwrap();
    store.record(&text("upper", 20)).unwrap();
    store.record(&text("lower", 30)).unwrap();
    assert_eq!(texts(&store.history(10).unwrap()), ["lower", "upper"]);
    store.record(&text("upper", 5)).unwrap();
    assert_eq!(texts(&store.history(10).unwrap()), ["upper", "lower"]);
    store.record(&text("lower", 5)).unwrap();
    assert_eq!(texts(&store.history(10).unwrap()), ["lower", "upper"]);
}

#[test]
fn an_over_cap_body_stays_as_the_only_row_and_older_rows_go() {
    let limits = Limits {
        max_history_text_bytes: 10,
        ..Limits::default()
    };
    let (_dir, mut store) = temp_store(limits);
    store.record(&text("aaaa", 1)).unwrap();
    store
        .record(&text("this body is far over the cap", 2))
        .unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(texts(&rows), ["this body is far over the cap"]);
    store.record(&text("bb", 3)).unwrap();
    assert_eq!(texts(&store.history(10).unwrap()), ["bb"]);
}

#[test]
fn get_reads_one_row_by_id_and_none_for_a_missing_id() {
    let (_dir, mut store) = temp_store(Limits::default());
    store.record(&text("one", 1)).unwrap();
    store
        .record(&path_entry(&["a.txt"], Transfer::Cut, 2))
        .unwrap();
    let rows = store.history(10).unwrap();
    assert_eq!(store.get(rows[0].id).unwrap().as_ref(), Some(&rows[0]));
    assert_eq!(store.get(rows[1].id).unwrap().as_ref(), Some(&rows[1]));
    assert_eq!(store.get(rows[0].id + rows[1].id + 100).unwrap(), None);
}
