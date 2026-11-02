use rusqlite::params;

use super::{item, temp_store};
use crate::limits::Limits;
use crate::model::SavedKind;
use crate::store::ClipStore;

const BAD_ROWS: [&str; 7] = [
    "INSERT INTO saved (kind, title, body, created_ms, updated_ms) VALUES (9, 't', 'b', 1, 1)",
    "INSERT INTO saved (kind, title, body, sealed, created_ms, updated_ms)
     VALUES (3, 't', 'plain', x'01', 1, 1)",
    "INSERT INTO saved (kind, title, sealed, created_ms, updated_ms) VALUES (3, 't', x'', 1, 1)",
    "INSERT INTO saved (kind, title, body, sealed, created_ms, updated_ms)
     VALUES (0, 't', 'b', x'01', 1, 1)",
    "INSERT INTO saved (kind, title, body, created_ms, updated_ms)
     VALUES (0, CAST(x'ff' AS TEXT), 'b', 1, 1)",
    "INSERT INTO saved (kind, title, body, scope, created_ms, updated_ms)
     VALUES (0, 't', 'b', CAST(x'c3' AS TEXT), 1, 1)",
    "INSERT INTO saved (kind, title, body, scope, created_ms, updated_ms)
     VALUES (0, 't', 'b', '', 1, 1)",
];

fn insert_raw(store: &ClipStore, sql: &str) -> i64 {
    store.conn.execute(sql, []).unwrap();
    store.conn.last_insert_rowid()
}

fn row_count(store: &ClipStore) -> i64 {
    store
        .conn
        .query_row("SELECT count(*) FROM saved", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn unreadable_rows_are_skipped_by_id_and_the_rest_still_list() {
    let (_dir, store) = temp_store(Limits::default());
    let first = store.save(&item(SavedKind::Snippet, "first")).unwrap();
    let bad: Vec<i64> = BAD_ROWS.iter().map(|sql| insert_raw(&store, sql)).collect();
    let last = store.save(&item(SavedKind::Command, "last")).unwrap();
    let listed = store.saved().unwrap();
    let ids: Vec<i64> = listed.items.iter().map(|saved| saved.id).collect();
    assert_eq!(ids, vec![first, last]);
    assert_eq!(listed.skipped, bad);
}

#[test]
fn a_skipped_row_can_be_deleted_by_its_id() {
    let (_dir, store) = temp_store(Limits::default());
    let kept = store.save(&item(SavedKind::Snippet, "kept")).unwrap();
    for sql in BAD_ROWS {
        insert_raw(&store, sql);
    }
    for id in store.saved().unwrap().skipped {
        store.delete_saved(id).unwrap();
    }
    let listed = store.saved().unwrap();
    assert!(listed.skipped.is_empty());
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].id, kept);
    assert_eq!(row_count(&store), 1);
}

#[test]
fn kind_and_times_cannot_be_stored_empty() {
    let (_dir, store) = temp_store(Limits::default());
    for (kind, created, updated) in [
        (None, Some(1), Some(1)),
        (Some(0), None, Some(1)),
        (Some(0), Some(1), None),
    ] {
        let inserted = store.conn.execute(
            "INSERT INTO saved (kind, title, body, created_ms, updated_ms)
             VALUES (?1, 't', 'b', ?2, ?3)",
            params![kind, created, updated],
        );
        assert!(inserted.is_err(), "{kind:?} {created:?} {updated:?}");
    }
    assert_eq!(row_count(&store), 0);
}
