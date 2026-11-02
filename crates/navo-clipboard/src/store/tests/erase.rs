use std::fs;

use rusqlite::{Connection, TransactionBehavior};
use tempfile::TempDir;

use super::safety::{assert_absent_everywhere, db_path, with_suffix};
use super::{folder, item, secret, text};
use crate::limits::Limits;
use crate::model::{Saved, SavedKind, Scope};
use crate::store::{ClipError, ClipStore};

fn open(dir: &TempDir) -> ClipStore {
    ClipStore::open(&db_path(dir), Limits::default()).unwrap()
}

fn assert_gone_while_open(dir: &TempDir, canary: &str) {
    let path = db_path(dir);
    assert!(path.exists());
    assert!(with_suffix(&path, "-shm").exists());
    assert_absent_everywhere(dir.path(), canary);
}

#[test]
fn deleting_a_history_row_empties_the_wal_at_once() {
    let dir = TempDir::new().unwrap();
    let mut store = open(&dir);
    store.record(&text("canary-deleted-history", 1)).unwrap();
    store.record(&text("kept", 2)).unwrap();
    let id = store.history(10).unwrap()[1].id;
    store.delete(id).unwrap();
    assert_gone_while_open(&dir, "canary-deleted-history");
    assert_eq!(store.history(10).unwrap().len(), 1);
}

#[test]
fn a_history_row_trimmed_by_record_leaves_no_trace() {
    let dir = TempDir::new().unwrap();
    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    let mut store = ClipStore::open(&db_path(&dir), limits).unwrap();
    store.record(&text("canary-trimmed-history", 1)).unwrap();
    store.record(&text("newer", 2)).unwrap();
    assert_gone_while_open(&dir, "canary-trimmed-history");
}

#[test]
fn deleting_a_saved_item_and_a_secret_empties_the_wal_at_once() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir);
    let plain = store
        .save(&item(SavedKind::Snippet, "canary-deleted-item"))
        .unwrap();
    let sealed = store.save(&secret(b"canary-deleted-sealed")).unwrap();
    store.delete_saved(plain).unwrap();
    assert_gone_while_open(&dir, "canary-deleted-item");
    store.delete_saved(sealed).unwrap();
    assert_gone_while_open(&dir, "canary-deleted-sealed");
}

#[test]
fn updating_a_saved_item_leaves_no_old_text() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir);
    let id = store
        .save(&item(SavedKind::Snippet, "canary-replaced-item"))
        .unwrap();
    let replaced = Saved {
        id,
        ..item(SavedKind::Snippet, "after")
    };
    store.update(&replaced).unwrap();
    assert_gone_while_open(&dir, "canary-replaced-item");
}

#[test]
fn clearing_or_overwriting_a_note_leaves_no_old_text() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir);
    store
        .set_note(&folder("/srv/a"), "canary-cleared-note", 1)
        .unwrap();
    store
        .set_note(&Scope::Global, "canary-replaced-note", 1)
        .unwrap();
    store.set_note(&folder("/srv/a"), "", 2).unwrap();
    assert_gone_while_open(&dir, "canary-cleared-note");
    store.set_note(&Scope::Global, "after", 2).unwrap();
    assert_gone_while_open(&dir, "canary-replaced-note");
}

#[test]
fn deleting_unreadable_notes_empties_the_wal_at_once() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir);
    store.set_note(&folder("/kept"), "kept", 1).unwrap();
    let raw = Connection::open(db_path(&dir)).unwrap();
    raw.execute(
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES ('/bad', CAST(x'c3' AS TEXT), 'canary-unreadable-note', 1)",
        [],
    )
    .unwrap();
    drop(raw);
    assert_eq!(store.delete_unreadable_notes().unwrap(), 1);
    assert_gone_while_open(&dir, "canary-unreadable-note");
}

#[test]
fn a_busy_checkpoint_is_reported_and_the_delete_stays_committed() {
    let dir = TempDir::new().unwrap();
    let mut store = open(&dir);
    store
        .record(&text("deleted while a reader waits", 1))
        .unwrap();
    let id = store.history(1).unwrap()[0].id;
    let mut reader = Connection::open(db_path(&dir)).unwrap();
    reader.busy_timeout(std::time::Duration::ZERO).unwrap();
    let pin = reader
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .unwrap();
    let _: i64 = pin
        .query_row("SELECT count(*) FROM clips", [], |row| row.get(0))
        .unwrap();
    store.record(&text("written after the reader", 2)).unwrap();
    let result = store.delete(id);
    assert!(matches!(result, Err(ClipError::CheckpointBusy)));
    assert!(store.get(id).unwrap().is_none());
    drop(pin);
    drop(store);
    let store = open(&dir);
    assert!(store.get(id).unwrap().is_none());
    assert!(fs::metadata(db_path(&dir)).is_ok());
}
