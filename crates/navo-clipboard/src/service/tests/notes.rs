use rusqlite::Connection;

use super::*;
use crate::model::Scope;
use crate::service::FailKind;

fn folder(path: &str) -> Scope {
    Scope::Folder(PathBuf::from(path))
}

#[test]
fn a_note_round_trips_and_the_snapshot_holds_its_header_only() {
    let h = plain();
    answer(
        h.service
            .set_note(folder("/work/A"), "canary-note".into(), 5),
    )
    .unwrap();
    answer(h.service.set_note(Scope::Global, "global".into(), 6)).unwrap();
    let snapshot = h.service.snapshot();
    let headers: Vec<_> = snapshot
        .notes
        .iter()
        .map(|note| (note.scope.clone(), note.updated_ms))
        .collect();
    assert_eq!(headers, [(Scope::Global, 6), (folder("/work/A"), 5)]);
    assert!(!format!("{snapshot:?} {:?}", snapshot.notes).contains("canary"));

    let body = answer(h.service.note(folder("/WORK/a/"))).unwrap();
    assert_eq!(body.as_deref(), Some("canary-note"));
    let listed = answer(h.service.notes()).unwrap();
    let bodies: Vec<_> = listed.notes.iter().map(|note| note.body.as_str()).collect();
    assert_eq!(bodies, ["global", "canary-note"]);
}

#[test]
fn a_note_search_answers_with_counts_and_the_active_folder_first() {
    let h = plain();
    answer(
        h.service
            .set_note(Scope::Global, "global canary-note".into(), 1),
    )
    .unwrap();
    answer(h.service.set_note(folder("/work/A"), "one\ntwo".into(), 2)).unwrap();
    answer(h.service.set_note(folder("/work/B"), "other".into(), 3)).unwrap();
    let all = answer(h.service.search_notes(
        String::new(),
        Some(PathBuf::from("/work/b")),
        "Global note".into(),
    ))
    .unwrap();
    let titles: Vec<_> = all.iter().map(|note| note.title.as_str()).collect();
    assert_eq!(titles, ["B", "Global note", "A"]);
    assert_eq!(all[2].lines, 2);
    let by_text = answer(
        h.service
            .search_notes("canary".into(), None, "Global note".into()),
    );
    let by_text = by_text.unwrap();
    assert_eq!(by_text.len(), 1);
    assert!(!format!("{by_text:?}").contains("canary"));
}

#[test]
fn an_empty_body_deletes_the_note() {
    let h = plain();
    answer(h.service.set_note(folder("/a"), "x".into(), 1)).unwrap();
    answer(h.service.set_note(folder("/a"), String::new(), 2)).unwrap();
    assert!(h.service.snapshot().notes.is_empty());
    assert_eq!(answer(h.service.note(folder("/a"))), Ok(None));
}

#[test]
fn a_note_moves_but_never_onto_another_note() {
    let h = plain();
    answer(h.service.set_note(folder("/old"), "moving".into(), 1)).unwrap();
    answer(h.service.set_note(folder("/taken"), "stays".into(), 1)).unwrap();
    let onto = h.service.move_note(folder("/old"), folder("/taken"), 2);
    assert_eq!(answer(onto), Err(FailKind::NoteExists));
    let gone = h.service.move_note(folder("/none"), folder("/new"), 2);
    assert_eq!(answer(gone), Err(FailKind::NotFound));

    answer(h.service.move_note(folder("/old"), folder("/new"), 3)).unwrap();
    let scopes: Vec<_> = h
        .service
        .snapshot()
        .notes
        .iter()
        .map(|n| n.scope.clone())
        .collect();
    assert_eq!(scopes, [folder("/new"), folder("/taken")]);
    let body = answer(h.service.note(folder("/new"))).unwrap();
    assert_eq!(body.as_deref(), Some("moving"));
}

#[test]
fn unreadable_notes_are_counted_and_deleted_on_request() {
    let h = plain();
    let conn = Connection::open(h.db()).unwrap();
    conn.busy_timeout(WAIT).unwrap();
    conn.execute(
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES ('/bad', '/bad', CAST(x'ff' AS TEXT), 1)",
        [],
    )
    .unwrap();
    drop(conn);
    answer(h.service.set_note(folder("/good"), "ok".into(), 1)).unwrap();
    assert_eq!(h.service.snapshot().notes_skipped, 1);
    assert_eq!(answer(h.service.delete_unreadable_notes()), Ok(1));
    let snapshot = h.service.snapshot();
    assert_eq!(snapshot.notes_skipped, 0);
    assert_eq!(snapshot.notes.len(), 1);
}

#[test]
fn a_refused_note_delete_answers_store_and_keeps_the_note() {
    let h = plain();
    answer(h.service.set_note(folder("/a"), "x".into(), 1)).unwrap();
    super::saved::refuse_deletes(&h, "notes");
    assert_eq!(
        answer(h.service.set_note(folder("/a"), String::new(), 2)),
        Err(FailKind::Store)
    );
    assert_eq!(answer(h.service.note(folder("/a"))), Ok(Some("x".into())));
}
