use super::{folder, temp_store};
use crate::limits::Limits;
use crate::model::{NoteRow, Scope};
use crate::store::{ClipError, ClipStore, NoteListed};

fn note_rows(store: &ClipStore) -> i64 {
    store
        .conn
        .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .unwrap()
}

fn insert_raw(store: &ClipStore, sql: &str) {
    store.conn.execute(sql, []).unwrap();
}

fn row(scope: Scope, body: &str, updated_ms: i64) -> NoteRow {
    NoteRow {
        scope,
        body: body.to_owned(),
        updated_ms,
    }
}

#[test]
fn one_folder_is_one_note_whatever_the_slash_trailing_separator_or_case() {
    let (_dir, store) = temp_store(Limits::default());
    for path in [r"C:\a", "C:/a", r"C:\a\", r"C:\A"] {
        store.set_note(&folder(path), path, 1).unwrap();
    }
    assert_eq!(note_rows(&store), 1);
    assert_eq!(
        store.note(&folder("c:/a/")).unwrap().as_deref(),
        Some(r"C:\A")
    );
}

#[test]
fn non_ascii_case_names_one_note() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder(r"C:\Übung"), "upper", 1).unwrap();
    store.set_note(&folder(r"c:\übung\"), "lower", 2).unwrap();
    assert_eq!(note_rows(&store), 1);
    assert_eq!(
        store.note(&folder("C:/ÜBUNG")).unwrap().as_deref(),
        Some("lower")
    );
}

#[test]
fn the_global_note_stays_apart_from_every_folder_and_a_root() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&Scope::Global, "global", 1).unwrap();
    store.set_note(&folder("/"), "root", 1).unwrap();
    store.set_note(&folder("/srv/a"), "folder a", 1).unwrap();
    assert_eq!(note_rows(&store), 3);
    assert_eq!(store.note(&folder("/srv/b")).unwrap(), None);
    assert_eq!(store.note(&folder(r"\")).unwrap().as_deref(), Some("root"));
    store.set_note(&folder("/SRV/A"), "", 2).unwrap();
    assert_eq!(store.note(&folder("/srv/a")).unwrap(), None);
    assert_eq!(
        store.note(&Scope::Global).unwrap().as_deref(),
        Some("global")
    );
    assert!(matches!(
        store.set_note(&folder(""), "x", 3),
        Err(ClipError::Malformed(_))
    ));
}

#[test]
fn notes_list_every_note_with_the_path_as_last_written() {
    let (_dir, store) = temp_store(Limits::default());
    store
        .set_note(&folder(r"Z:\Gone\Project"), "gone", 1)
        .unwrap();
    store
        .set_note(&folder(r"z:/gone/project/"), "still", 2)
        .unwrap();
    store.set_note(&Scope::Global, "global", 3).unwrap();
    store
        .set_note(&folder(r"\\server\Share"), "share", 4)
        .unwrap();
    assert_eq!(
        store.notes().unwrap().notes,
        vec![
            row(Scope::Global, "global", 3),
            row(folder(r"\\server\Share"), "share", 4),
            row(folder("z:/gone/project/"), "still", 2),
        ]
    );
}

#[test]
fn a_note_moves_to_a_folder_without_one() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder(r"C:\Old"), "kept", 1).unwrap();
    store
        .move_note(&folder("c:/old/"), &folder(r"D:\New"), 5)
        .unwrap();
    assert_eq!(store.note(&folder(r"C:\Old")).unwrap(), None);
    assert_eq!(
        store.notes().unwrap().notes,
        vec![row(folder(r"D:\New"), "kept", 5)]
    );
}

#[test]
fn a_note_moves_onto_another_spelling_of_its_own_folder() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder(r"C:\proj"), "kept", 1).unwrap();
    store
        .move_note(&folder(r"C:\proj"), &folder(r"C:\Proj"), 2)
        .unwrap();
    assert_eq!(
        store.notes().unwrap().notes,
        vec![row(folder(r"C:\Proj"), "kept", 2)]
    );
}

#[test]
fn moving_onto_an_existing_note_is_refused_and_both_stay() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder("/a"), "a", 1).unwrap();
    store.set_note(&folder("/b"), "b", 1).unwrap();
    store.set_note(&Scope::Global, "g", 1).unwrap();
    assert!(matches!(
        store.move_note(&folder("/a"), &folder("/B/"), 2),
        Err(ClipError::NoteExists)
    ));
    assert_eq!(
        store.notes().unwrap().notes,
        vec![
            row(Scope::Global, "g", 1),
            row(folder("/a"), "a", 1),
            row(folder("/b"), "b", 1),
        ]
    );
}

#[test]
fn moving_a_missing_note_is_refused_and_creates_nothing() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder("/b"), "b", 1).unwrap();
    assert!(matches!(
        store.move_note(&folder("/a"), &folder("/c"), 2),
        Err(ClipError::NoteMissing)
    ));
    assert_eq!(
        store.notes().unwrap().notes,
        vec![row(folder("/b"), "b", 1)]
    );
}

#[test]
fn an_over_cap_folder_path_is_refused_for_a_note() {
    let limits = Limits {
        max_saved_bytes: 8,
        ..Limits::default()
    };
    let (_dir, store) = temp_store(limits);
    store.set_note(&folder("/1234567"), "kept", 1).unwrap();
    let long = folder("/12345678");
    assert!(matches!(
        store.set_note(&long, "x", 2),
        Err(ClipError::TooLarge { bytes: 9, cap: 8 })
    ));
    assert!(matches!(
        store.move_note(&folder("/1234567"), &long, 2),
        Err(ClipError::TooLarge { .. })
    ));
    assert_eq!(
        store.notes().unwrap().notes,
        vec![row(folder("/1234567"), "kept", 1)]
    );
}

#[test]
fn only_a_folder_note_moves_to_a_folder() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder("/a"), "a", 1).unwrap();
    store.set_note(&Scope::Global, "g", 1).unwrap();
    for (from, to) in [
        (folder("/a"), Scope::Global),
        (Scope::Global, folder("/c")),
        (Scope::Global, Scope::Global),
    ] {
        assert!(matches!(
            store.move_note(&from, &to, 2),
            Err(ClipError::Malformed(_))
        ));
    }
    assert_eq!(
        store.notes().unwrap().notes,
        vec![row(Scope::Global, "g", 1), row(folder("/a"), "a", 1)]
    );
}

#[test]
fn an_unreadable_note_is_skipped_and_the_rest_still_list() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder("/first"), "first", 1).unwrap();
    insert_raw(
        &store,
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES ('/bad-body', '/bad-body', CAST(x'ff' AS TEXT), 1)",
    );
    insert_raw(
        &store,
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES ('/bad-name', CAST(x'c3' AS TEXT), 'x', 1)",
    );
    insert_raw(
        &store,
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES (CAST(x'80' AS TEXT), '/bad-key', 'x', 1)",
    );
    store.set_note(&Scope::Global, "global", 2).unwrap();
    assert_eq!(
        store.notes().unwrap(),
        NoteListed {
            notes: vec![
                row(Scope::Global, "global", 2),
                row(folder("/first"), "first", 1)
            ],
            skipped: 3,
        }
    );
}

#[test]
fn unreadable_notes_can_be_deleted_and_readable_ones_stay() {
    let (_dir, store) = temp_store(Limits::default());
    store.set_note(&folder("/kept"), "kept", 1).unwrap();
    insert_raw(
        &store,
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES ('/bad', '/bad', CAST(x'ff' AS TEXT), 1)",
    );
    assert_eq!(store.delete_unreadable_notes().unwrap(), 1);
    assert_eq!(store.delete_unreadable_notes().unwrap(), 0);
    assert_eq!(
        store.notes().unwrap(),
        NoteListed {
            notes: vec![row(folder("/kept"), "kept", 1)],
            skipped: 0,
        }
    );
    assert_eq!(note_rows(&store), 1);
}

#[test]
fn a_global_note_with_a_damaged_display_stays_listed_and_is_not_deleted() {
    let (_dir, store) = temp_store(Limits::default());
    insert_raw(
        &store,
        "INSERT INTO notes (key, display, body, updated_ms)
         VALUES ('', CAST(x'c3' AS TEXT), 'global', 3)",
    );
    assert_eq!(
        store.notes().unwrap(),
        NoteListed {
            notes: vec![row(Scope::Global, "global", 3)],
            skipped: 0,
        }
    );
    assert_eq!(store.delete_unreadable_notes().unwrap(), 0);
    assert_eq!(note_rows(&store), 1);
}
