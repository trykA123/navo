use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tempfile::TempDir;

use super::safety::{assert_absent_everywhere, db_path, names};
use super::{item, secret, set_fail_cleanup, text};
use crate::limits::Limits;
use crate::model::{Content, SavedKind};
use crate::store::{ClipError, ClipStore, Opened, V1, V2};

const SEALED: &[u8] = b"\x00\xffsealed-bytes\x01";

/// A V2 file as an older Navo left it: ids 3 and 7 in history, 5 and 9 in saved, one note.
fn v2_file(dir: &TempDir) -> PathBuf {
    let path = db_path(dir);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
    conn.execute_batch(V1).unwrap();
    conn.execute_batch(V2).unwrap();
    conn.execute_batch(
        "INSERT INTO clips (id, key, kind, body, paths, cut, source_app, first_ms, last_ms, uses)
         VALUES (3, x'03', 0, 'third', NULL, 0, 'a.exe', 10, 10, 1),
                (7, x'07', 0, 'seventh', NULL, 0, NULL, 20, 20, 2);
         INSERT INTO saved (id, kind, title, body, user, sealed, scope, created_ms, updated_ms)
         VALUES (5, 0, 'snippet', 'snippet body', NULL, NULL, NULL, 1, 2);
         INSERT INTO notes (key, display, body, updated_ms) VALUES ('k', 'd', 'note body', 4);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO saved (id, kind, title, body, user, sealed, scope, created_ms, updated_ms)
         VALUES (9, 3, 'secret', NULL, 'admin', ?1, NULL, 3, 4)",
        [SEALED],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    path
}

fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

fn user_version(path: &Path) -> u32 {
    raw(path)
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn sequence(path: &Path, table: &str) -> Option<i64> {
    raw(path)
        .query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = ?1",
            [table],
            |row| row.get(0),
        )
        .ok()
}

fn history_ids(store: &ClipStore) -> Vec<i64> {
    store
        .history(10)
        .unwrap()
        .iter()
        .map(|row| row.id)
        .collect()
}

fn saved_ids(store: &ClipStore) -> Vec<i64> {
    store.saved().unwrap().items.iter().map(|i| i.id).collect()
}

#[test]
fn a_v2_file_upgrades_to_v3_and_keeps_every_id_row_and_sealed_byte() {
    let dir = TempDir::new().unwrap();
    let path = v2_file(&dir);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.opened(), &Opened::Clean);
    let history = store.history(10).unwrap();
    assert_eq!(history_ids(&store), [7, 3]);
    assert_eq!(history[1].entry.content, Content::Text("third".into()));
    assert_eq!(history[0].entry.uses, 2);
    let saved = store.saved().unwrap().items;
    assert_eq!(saved_ids(&store), [5, 9]);
    assert_eq!(saved[0].body.as_deref(), Some("snippet body"));
    assert_eq!(saved[1].kind, SavedKind::Secret);
    assert_eq!(saved[1].user.as_deref(), Some("admin"));
    assert_eq!(saved[1].sealed.as_ref().unwrap().0, SEALED);
    drop(store);
    assert_eq!(user_version(&path), 3);
    assert_eq!(sequence(&path, "clips"), Some(7));
    assert_eq!(sequence(&path, "saved"), Some(9));
    let note: String = raw(&path)
        .query_row("SELECT body FROM notes WHERE key = 'k'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(note, "note body");
}

#[test]
fn the_v3_tables_keep_their_index_and_unique_key() {
    let dir = TempDir::new().unwrap();
    drop(ClipStore::open(&v2_file(&dir), Limits::default()).unwrap());
    let conn = raw(&db_path(&dir));
    let indexed: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'clips_recent' AND tbl_name = 'clips'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let leftovers: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name LIKE '%_v2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((indexed, leftovers), (1, 0));
    let duplicate = conn.execute(
        "INSERT INTO clips (key, kind, body, cut, first_ms, last_ms, uses)
         VALUES (x'03', 0, 'x', 0, 1, 1, 1)",
        [],
    );
    assert!(duplicate.is_err(), "the content key stays unique");
}

#[test]
fn a_deleted_newest_history_id_is_never_issued_again() {
    let dir = TempDir::new().unwrap();
    let mut store = ClipStore::open(&db_path(&dir), Limits::default()).unwrap();
    store.record(&text("first", 1)).unwrap();
    store.record(&text("newest", 2)).unwrap();
    let newest = history_ids(&store)[0];
    store.delete(newest).unwrap();
    drop(store);
    let mut store = ClipStore::open(&db_path(&dir), Limits::default()).unwrap();
    store.record(&text("after the delete", 3)).unwrap();
    let after = history_ids(&store)[0];
    assert!(after > newest, "{after} must be above the deleted {newest}");
    assert!(store.get(newest).unwrap().is_none());
}

#[test]
fn a_deleted_newest_saved_id_is_never_issued_again() {
    let dir = TempDir::new().unwrap();
    let store = ClipStore::open(&db_path(&dir), Limits::default()).unwrap();
    store.save(&item(SavedKind::Snippet, "older")).unwrap();
    let newest = store.save(&secret(b"sealed")).unwrap();
    store.delete_saved(newest).unwrap();
    drop(store);
    let store = ClipStore::open(&db_path(&dir), Limits::default()).unwrap();
    let after = store.save(&item(SavedKind::Snippet, "new")).unwrap();
    assert!(after > newest, "{after} must be above the deleted {newest}");
}

#[test]
fn a_migrated_file_issues_ids_above_its_highest_row() {
    let dir = TempDir::new().unwrap();
    let mut store = ClipStore::open(&v2_file(&dir), Limits::default()).unwrap();
    store.delete_saved(9).unwrap();
    assert_eq!(store.save(&item(SavedKind::Snippet, "new")).unwrap(), 10);
    store.delete(7).unwrap();
    store.record(&text("new row", 30)).unwrap();
    assert_eq!(history_ids(&store)[0], 8);
}

#[test]
fn a_v1_file_migrates_through_v2_to_v3_with_its_ids() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(V1).unwrap();
    conn.execute_batch(
        "INSERT INTO clips (id, key, kind, body, paths, cut, source_app, first_ms, last_ms, uses)
         VALUES (4, x'04', 0, 'four', NULL, 0, NULL, 10, 10, 1),
                (6, x'06', 0, 'six', NULL, 0, NULL, 20, 20, 1);",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    drop(conn);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(history_ids(&store), [6, 4]);
    let id = store
        .save(&item(SavedKind::Snippet, "first saved"))
        .unwrap();
    assert_eq!(id, 1);
    drop(store);
    assert_eq!(user_version(&path), 3);
    assert_eq!(sequence(&path, "clips"), Some(6));
}

#[test]
fn a_failed_v3_upgrade_leaves_the_v2_file_untouched() {
    let dir = TempDir::new().unwrap();
    let path = v2_file(&dir);
    raw(&path)
        .execute_batch("CREATE TABLE saved_v2 (blocker TEXT);")
        .unwrap();
    let before = fs::read(&path).unwrap();
    let result = ClipStore::open(&path, Limits::default());
    assert!(matches!(result, Err(ClipError::Sqlite(_))));
    assert!(!names(dir.path()).iter().any(|n| n.contains("corrupt")));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(user_version(&path), 2);
    let conn = raw(&path);
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM clips ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(ids, [3, 7]);
    let autoincrement: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'sqlite_sequence'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(autoincrement, 0);
}

const STALE: [&str; 3] = [
    "canary-deleted-long",
    "canary-deleted-title",
    "canary-deleted-body",
];

fn holds(path: &Path, needle: &str) -> bool {
    let bytes = fs::read(path).unwrap();
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// A V2 file whose deleted rows still sit in free pages, as a writer without secure_delete
/// leaves them.
fn v2_file_with_stale_text(dir: &TempDir) -> PathBuf {
    let path = v2_file(dir);
    let conn = raw(&path);
    conn.execute_batch("PRAGMA secure_delete = OFF;").unwrap();
    let long = "canary-deleted-long ".repeat(4000);
    conn.execute(
        "INSERT INTO clips (id, key, kind, body, cut, first_ms, last_ms, uses)
         VALUES (8, x'08', 0, ?1, 0, 30, 30, 1)",
        [&long],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO saved (id, kind, title, body, created_ms, updated_ms)
         VALUES (11, 0, 'canary-deleted-title', 'canary-deleted-body', 1, 1);
         DELETE FROM clips WHERE id = 8;
         DELETE FROM saved WHERE id = 11;",
    )
    .unwrap();
    drop(conn);
    assert!(holds(&path, STALE[0]), "the test needs the stale text");
    assert!(holds(&path, STALE[1]), "the test needs the stale text");
    path
}

#[test]
fn the_upgraded_file_holds_no_text_of_rows_deleted_before_it() {
    let dir = TempDir::new().unwrap();
    let path = v2_file_with_stale_text(&dir);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    for canary in STALE {
        assert_absent_everywhere(dir.path(), canary);
    }
    assert_eq!(history_ids(&store), [7, 3]);
}

#[test]
fn an_upgrade_whose_cleanup_fails_is_cleaned_on_a_later_open() {
    let dir = TempDir::new().unwrap();
    let path = v2_file_with_stale_text(&dir);
    set_fail_cleanup(true);
    for _ in 0..2 {
        let mut store = ClipStore::open(&path, Limits::default()).unwrap();
        assert!(store.cleanup_owed().unwrap());
        assert_eq!(history_ids(&store), [7, 3]);
        store.record(&text("works while owed", 40)).unwrap();
        let newest = history_ids(&store)[0];
        store.delete(newest).unwrap();
        drop(store);
        assert!(holds(&path, STALE[0]), "the cleanup is still owed");
    }
    set_fail_cleanup(false);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert!(!store.cleanup_owed().unwrap());
    for canary in STALE {
        assert_absent_everywhere(dir.path(), canary);
    }
    assert_eq!(history_ids(&store), [7, 3]);
    assert_eq!(saved_ids(&store), [5, 9]);
    drop(store);
    assert_eq!(user_version(&path), 3);
    let marks: i64 = raw(&path)
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'upgrade_cleanup'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marks, 0);
}

#[test]
fn ids_keep_rising_across_delete_all() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    store.record(&text("one", 1)).unwrap();
    store.record(&text("two", 2)).unwrap();
    let clip = history_ids(&store)[0];
    let saved = store.save(&secret(b"sealed")).unwrap();
    store.delete(clip).unwrap();
    let mut store = store.wipe_all(&path).unwrap();
    store.record(&text("after", 3)).unwrap();
    assert!(history_ids(&store)[0] > clip);
    let after = store.save(&item(SavedKind::Snippet, "after")).unwrap();
    assert!(after > saved, "{after} must be above {saved}");
    let store = store.wipe_all(&path).unwrap();
    assert!(store.save(&item(SavedKind::Snippet, "again")).unwrap() > after);
}
