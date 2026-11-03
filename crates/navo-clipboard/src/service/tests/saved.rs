use rusqlite::Connection;

use super::secrets::{secret, secret_item};
use super::*;
use crate::model::{Saved, SavedKind};
use crate::service::FailKind;

fn snippet(title: &str, body: &str) -> Saved {
    Saved {
        kind: SavedKind::Snippet,
        body: Some(body.to_owned()),
        user: None,
        ..secret_item(title)
    }
}

#[test]
fn save_update_and_delete_show_in_the_next_snapshot() {
    let h = plain();
    let id = answer(h.service.save(snippet("Greeting", "hello"), None)).unwrap();
    let snapshot = wait_for(&h.service, |s| s.saved.len() == 1);
    assert_eq!(snapshot.saved[0].id, id);
    assert_eq!(snapshot.saved[0].body.as_deref(), Some("hello"));

    let changed = Saved {
        id,
        ..snippet("Greeting", "hello there")
    };
    answer(h.service.update(changed, None)).unwrap();
    let after = h.service.snapshot();
    assert_eq!(after.saved[0].body.as_deref(), Some("hello there"));
    assert!(after.revision > snapshot.revision);

    assert_eq!(answer(h.service.delete_saved(id)), Ok(()));
    wait_for(&h.service, |s| s.saved.is_empty());
}

#[test]
fn an_update_of_a_missing_item_fails_as_not_found() {
    let h = plain();
    let missing = Saved {
        id: 42,
        ..snippet("Gone", "x")
    };
    assert_eq!(
        answer(h.service.update(missing, None)),
        Err(FailKind::NotFound)
    );
}

#[test]
fn an_oversized_body_fails_as_too_large() {
    let limits = Limits {
        max_saved_bytes: 8,
        ..Limits::default()
    };
    let h = harness(FakeSink::default(), limits);
    let item = snippet("Big", "more than eight bytes");
    assert_eq!(answer(h.service.save(item, None)), Err(FailKind::TooLarge));
    assert!(h.service.snapshot().saved.is_empty());
}

#[test]
fn save_from_history_copies_text_and_keeps_the_row() {
    let h = plain();
    let id = first_id(&h, "\n  first line\nsecond line");
    let saved_id = answer(h.service.save_from_history(id, 7)).unwrap();
    let snapshot = wait_for(&h.service, |s| s.saved.len() == 1);
    let item = &snapshot.saved[0];
    assert_eq!(item.id, saved_id);
    assert_eq!(item.kind, SavedKind::Snippet);
    assert_eq!(item.title, "first line");
    assert_eq!(item.body.as_deref(), Some("\n  first line\nsecond line"));
    assert_eq!((item.created_ms, item.updated_ms), (7, 7));
    assert_eq!(snapshot.history.len(), 1);
    assert_eq!(snapshot.history[0].id, id);
}

#[test]
fn save_from_history_turns_a_file_list_into_a_path_item() {
    let h = plain();
    h.service
        .captured(files(&["/a/one.txt", "/b/two.txt"], Transfer::Copy, 1));
    let id = wait_for(&h.service, |s| s.history.len() == 1).history[0].id;
    answer(h.service.save_from_history(id, 2)).unwrap();
    let snapshot = wait_for(&h.service, |s| s.saved.len() == 1);
    assert_eq!(snapshot.saved[0].kind, SavedKind::Path);
    assert_eq!(
        snapshot.saved[0].body.as_deref(),
        Some("/a/one.txt\n/b/two.txt")
    );
    assert_eq!(snapshot.saved[0].title, "/a/one.txt");
}

#[test]
fn save_from_history_of_an_unknown_row_fails() {
    let h = plain();
    assert_eq!(
        answer(h.service.save_from_history(999, 1)),
        Err(FailKind::NotFound)
    );
    assert!(h.service.snapshot().saved.is_empty());
}

#[test]
fn copying_a_plain_item_writes_its_text_and_starts_no_clear() {
    let h = plain();
    let id = answer(h.service.save(snippet("Greeting", "hello"), None)).unwrap();
    answer(h.service.copy_saved(id, How::Paste)).unwrap();
    assert_eq!(h.sink.calls(), [Call::Text("hello".into())]);
    h.advance(Duration::from_secs(60));
    answer(h.service.notes()).unwrap();
    assert!(h.sink.clears().is_empty());
}

#[test]
fn an_unreadable_saved_row_is_counted_and_can_be_deleted() {
    let h = plain();
    let conn = Connection::open(h.db()).unwrap();
    conn.busy_timeout(WAIT).unwrap();
    conn.execute(
        "INSERT INTO saved (id, kind, title, created_ms, updated_ms) VALUES (50, 99, 'x', 1, 1)",
        [],
    )
    .unwrap();
    drop(conn);
    answer(h.service.save(snippet("Good", "ok"), None)).unwrap();
    let snapshot = h.service.snapshot();
    assert_eq!(snapshot.saved.len(), 1);
    assert_eq!(snapshot.saved_skipped, 1);
    assert_eq!(answer(h.service.delete_saved(50)), Ok(()));
    wait_for(&h.service, |s| s.saved_skipped == 0);
}

#[test]
fn a_request_after_stop_answers_busy() {
    let h = plain();
    assert!(h.service.stop());
    assert_eq!(
        answer(h.service.save(snippet("Late", "x"), None)),
        Err(FailKind::Busy)
    );
    assert_eq!(answer(h.service.delete_saved(1)), Err(FailKind::Busy));
}

/// Makes every delete from `table` fail, the way a locked or read-only file would.
pub(super) fn refuse_deletes(h: &Harness, table: &str) {
    let conn = Connection::open(h.db()).unwrap();
    conn.busy_timeout(WAIT).unwrap();
    conn.execute_batch(&format!(
        "CREATE TRIGGER refuse_delete BEFORE DELETE ON {table}
         BEGIN SELECT RAISE(ABORT, 'refused'); END;"
    ))
    .unwrap();
}

#[test]
fn a_refused_delete_of_a_saved_item_or_secret_answers_store_and_keeps_both() {
    let h = plain();
    let plain_id = answer(h.service.save(snippet("Greeting", "hello"), None)).unwrap();
    let secret_id = answer(h.service.save(secret_item("Mail"), secret("pw"))).unwrap();
    refuse_deletes(&h, "saved");
    assert_eq!(
        answer(h.service.delete_saved(plain_id)),
        Err(FailKind::Store)
    );
    assert_eq!(
        answer(h.service.delete_saved(secret_id)),
        Err(FailKind::Store)
    );
    assert_eq!(h.service.snapshot().saved.len(), 2);
}

#[test]
fn a_delete_whose_log_cannot_be_emptied_answers_store() {
    let h = plain();
    let id = answer(h.service.save(secret_item("Mail"), secret("pw"))).unwrap();
    let mut reader = Connection::open(h.db()).unwrap();
    let pin = reader.transaction().unwrap();
    let _: i64 = pin
        .query_row("SELECT count(*) FROM saved", [], |row| row.get(0))
        .unwrap();
    answer(h.service.save(snippet("After", "x"), None)).unwrap();
    let started = std::time::Instant::now();
    let deleted = h.service.delete_saved(id).recv_timeout(WAIT).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "before the hook gives up"
    );
    drop(pin);
    assert_eq!(deleted, Err(FailKind::Store));
}
