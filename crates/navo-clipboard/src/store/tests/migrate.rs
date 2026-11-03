use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::config::DbConfig;
use tempfile::TempDir;

use super::safety::names;
use crate::limits::Limits;
use crate::model::{Content, Saved, SavedKind, Scope};
use crate::store::{ClipError, ClipStore, Opened, V1};

fn v1_file(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("clipboard.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(V1).unwrap();
    conn.execute_batch(
        "INSERT INTO clips (key, kind, body, paths, cut, source_app, first_ms, last_ms, uses)
         VALUES (x'01', 0, 'older text', NULL, 0, 'a.exe', 10, 10, 1),
                (x'02', 1, NULL, 'C:\\a' || char(0) || '/b', 1, NULL, 20, 20, 3);",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    path
}

fn v1_wal_file_with_unflushed_rows(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("clipboard.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;")
        .unwrap();
    conn.execute_batch(V1).unwrap();
    conn.execute_batch(
        "INSERT INTO clips (key, kind, body, paths, cut, source_app, first_ms, last_ms, uses)
         VALUES (x'01', 0, 'wal text one', NULL, 0, NULL, 10, 10, 1),
                (x'02', 0, 'wal text two', NULL, 0, NULL, 20, 20, 2);",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .unwrap();
    drop(conn);
    path
}

fn user_version(path: &Path) -> u32 {
    let conn = Connection::open(path).unwrap();
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn snippet() -> Saved {
    Saved {
        id: 0,
        kind: SavedKind::Snippet,
        title: "kept".into(),
        body: Some("kept body".into()),
        user: None,
        sealed: None,
        scope: Scope::Global,
        created_ms: 1,
        updated_ms: 1,
    }
}

#[test]
fn a_v1_file_upgrades_to_v3_and_keeps_every_history_row() {
    let dir = TempDir::new().unwrap();
    let path = v1_file(&dir);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.opened(), &Opened::Clean);
    let rows = store.history(10).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].entry.uses, 3);
    assert_eq!(rows[1].entry.content, Content::Text("older text".into()));
    assert!(store.saved().unwrap().items.is_empty());
    assert_eq!(store.note(&Scope::Global).unwrap(), None);
    drop(store);
    assert_eq!(user_version(&path), 3);
}

#[test]
fn a_v3_file_reopens_without_migrating_again() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("clipboard.db");
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    let id = store.save(&snippet()).unwrap();
    store.set_note(&Scope::Global, "kept note", 1).unwrap();
    drop(store);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.saved().unwrap().items[0].id, id);
    assert_eq!(
        store.note(&Scope::Global).unwrap().as_deref(),
        Some("kept note")
    );
    drop(store);
    assert_eq!(user_version(&path), 3);
}

#[test]
fn a_failed_upgrade_leaves_the_v1_file_and_its_rows_in_place() {
    let dir = TempDir::new().unwrap();
    let path = v1_file(&dir);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE notes (blocker TEXT);")
        .unwrap();
    drop(conn);
    let result = ClipStore::open(&path, Limits::default());
    assert!(matches!(result, Err(ClipError::Sqlite(_))));
    assert!(
        !names(dir.path())
            .iter()
            .any(|name| name.contains("corrupt"))
    );
    assert_eq!(user_version(&path), 1);
    let conn = Connection::open(&path).unwrap();
    let saved_tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'saved'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let clips: i64 = conn
        .query_row("SELECT count(*) FROM clips", [], |row| row.get(0))
        .unwrap();
    assert_eq!((saved_tables, clips), (0, 2));
}

#[test]
fn a_version_4_file_is_refused_untouched() {
    let dir = TempDir::new().unwrap();
    let path = v1_file(&dir);
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();
    drop(conn);
    let before = fs::read(&path).unwrap();
    let listing = names(dir.path());
    let result = ClipStore::open(&path, Limits::default());
    assert!(matches!(
        result,
        Err(ClipError::TooNew {
            found: 4,
            supported: 3
        })
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(names(dir.path()), listing);
}

#[test]
fn a_v1_wal_file_with_uncheckpointed_rows_upgrades_and_keeps_them() {
    let dir = TempDir::new().unwrap();
    let path = v1_wal_file_with_unflushed_rows(&dir);
    let main_only = TempDir::new().unwrap();
    fs::copy(&path, main_only.path().join("clipboard.db")).unwrap();
    assert_eq!(user_version(&main_only.path().join("clipboard.db")), 0);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.opened(), &Opened::Clean);
    let bodies: Vec<Content> = store
        .history(10)
        .unwrap()
        .into_iter()
        .map(|stored| stored.entry.content)
        .collect();
    assert_eq!(
        bodies,
        vec![
            Content::Text("wal text two".into()),
            Content::Text("wal text one".into()),
        ]
    );
    store.save(&snippet()).unwrap();
    drop(store);
    assert_eq!(user_version(&path), 3);
    let reopened = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(reopened.history(10).unwrap().len(), 2);
    assert_eq!(reopened.saved().unwrap().items.len(), 1);
}
