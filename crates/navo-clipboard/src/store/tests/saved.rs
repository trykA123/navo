use super::{folder, item, secret, temp_store, text};
use crate::limits::{Limits, MAX_TITLE_BYTES};
use crate::model::{Saved, SavedKind, Scope, Sealed};
use crate::store::ClipError;

fn small_cap() -> Limits {
    Limits {
        max_saved_bytes: 8,
        ..Limits::default()
    }
}

#[test]
fn every_kind_round_trips_with_its_fields() {
    let (_dir, store) = temp_store(Limits::default());
    let mut expected = Vec::new();
    for kind in [
        SavedKind::Snippet,
        SavedKind::Command,
        SavedKind::Path,
        SavedKind::Note,
    ] {
        expected.push(item(kind, &format!("{kind:?} body\nline two")));
    }
    expected.push(secret(&[0, 1, 2, 255]));
    for saved in &mut expected {
        saved.id = store.save(saved).unwrap();
    }
    assert_eq!(store.saved().unwrap().items, expected);
}

#[test]
fn folder_scopes_round_trip_with_either_separator() {
    let (_dir, store) = temp_store(Limits::default());
    let scopes = [
        folder(r"C:\Work\Project"),
        folder("/home/admin/project"),
        folder(r"\\server\share\team"),
        folder(r"D:/mixed\separators/x"),
        Scope::Global,
    ];
    for scope in &scopes {
        let saved = Saved {
            scope: scope.clone(),
            ..item(SavedKind::Path, "p")
        };
        store.save(&saved).unwrap();
    }
    let stored: Vec<Scope> = store
        .saved()
        .unwrap()
        .items
        .into_iter()
        .map(|s| s.scope)
        .collect();
    assert_eq!(stored, scopes);
}

#[test]
fn a_secret_without_sealed_bytes_is_refused_and_nothing_is_stored() {
    let (_dir, store) = temp_store(Limits::default());
    let unsealed = Saved {
        sealed: None,
        ..secret(b"x")
    };
    assert!(matches!(
        store.save(&unsealed),
        Err(ClipError::SecretNotSealed)
    ));
    let id = store.save(&secret(b"sealed")).unwrap();
    let unsealed = Saved { id, ..unsealed };
    assert!(matches!(
        store.update(&unsealed),
        Err(ClipError::SecretNotSealed)
    ));
    let stored = store.saved().unwrap().items;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].sealed, Some(Sealed(b"sealed".to_vec())));
}

#[test]
fn a_secret_with_plain_text_is_refused() {
    let (_dir, store) = temp_store(Limits::default());
    let plain = Saved {
        body: Some("plain".into()),
        ..secret(b"sealed")
    };
    assert!(matches!(store.save(&plain), Err(ClipError::PlainSecret)));
    assert!(store.saved().unwrap().items.is_empty());
}

#[test]
fn sealed_bytes_on_a_plain_item_are_refused() {
    let (_dir, store) = temp_store(Limits::default());
    let odd = Saved {
        sealed: Some(Sealed(b"x".to_vec())),
        ..item(SavedKind::Snippet, "body")
    };
    assert!(matches!(store.save(&odd), Err(ClipError::Malformed(_))));
    assert!(store.saved().unwrap().items.is_empty());
}

#[test]
fn update_takes_the_callers_updated_ms_and_keeps_created_ms() {
    let (_dir, store) = temp_store(Limits::default());
    let id = store.save(&item(SavedKind::Command, "old")).unwrap();
    let edited = Saved {
        id,
        title: "new title".into(),
        body: Some("new".into()),
        scope: folder(r"C:\New"),
        created_ms: 999,
        updated_ms: 50,
        ..item(SavedKind::Snippet, "")
    };
    store.update(&edited).unwrap();
    let stored = store.saved().unwrap().items;
    assert_eq!(
        stored,
        vec![Saved {
            created_ms: 100,
            ..edited
        }]
    );
}

#[test]
fn updating_a_deleted_item_reports_it_missing() {
    let (_dir, store) = temp_store(Limits::default());
    let keep = store.save(&item(SavedKind::Snippet, "keep")).unwrap();
    let gone = store.save(&item(SavedKind::Snippet, "gone")).unwrap();
    store.delete_saved(gone).unwrap();
    store.delete_saved(gone).unwrap();
    let stale = Saved {
        id: gone,
        ..item(SavedKind::Snippet, "edit")
    };
    assert!(matches!(store.update(&stale), Err(ClipError::Missing)));
    let ids: Vec<i64> = store.saved().unwrap().items.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![keep]);
}

#[test]
fn over_cap_body_sealed_and_note_are_refused_and_at_cap_is_kept() {
    let (_dir, store) = temp_store(small_cap());
    let over = item(SavedKind::Snippet, "123456789");
    assert!(matches!(
        store.save(&over),
        Err(ClipError::TooLarge { bytes: 9, cap: 8 })
    ));
    assert!(matches!(
        store.save(&secret(b"123456789")),
        Err(ClipError::TooLarge { .. })
    ));
    let id = store.save(&item(SavedKind::Snippet, "12345678")).unwrap();
    let grown = Saved { id, ..over.clone() };
    assert!(matches!(
        store.update(&grown),
        Err(ClipError::TooLarge { .. })
    ));
    assert_eq!(
        store.saved().unwrap().items[0].body.as_deref(),
        Some("12345678")
    );
    store.set_note(&Scope::Global, "12345678", 1).unwrap();
    assert!(matches!(
        store.set_note(&Scope::Global, "ăăăăă", 2),
        Err(ClipError::TooLarge { bytes: 10, .. })
    ));
    assert_eq!(
        store.note(&Scope::Global).unwrap().as_deref(),
        Some("12345678")
    );
}

#[test]
fn clear_history_keeps_saved_items_and_notes() {
    let (_dir, mut store) = temp_store(Limits::default());
    store.record(&text("history", 1)).unwrap();
    store.save(&item(SavedKind::Snippet, "kept")).unwrap();
    store.set_note(&Scope::Global, "kept note", 1).unwrap();
    store.clear_history().unwrap();
    assert!(store.history(10).unwrap().is_empty());
    assert_eq!(store.saved().unwrap().items.len(), 1);
    assert!(store.note(&Scope::Global).unwrap().is_some());
}

#[test]
fn errors_and_debug_output_carry_no_text() {
    let saved = Saved {
        title: "canary-title".into(),
        user: Some("canary-user".into()),
        scope: folder("canary-folder"),
        ..secret(b"canary-sealed")
    };
    let errors = [
        ClipError::SecretNotSealed,
        ClipError::PlainSecret,
        ClipError::TooLarge { bytes: 9, cap: 8 },
        ClipError::Missing,
        ClipError::TooManySaved { cap: 1 },
        ClipError::NoteMissing,
        ClipError::NoteExists,
    ];
    let shown = format!("{saved:?} {:?} {errors:?}", saved.sealed);
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(!shown.contains("canary"), "{shown}");
    assert!(shown.contains("Sealed(len 13)"), "{shown}");
    assert!(messages.iter().all(|message| !message.contains("canary")));
}

#[test]
fn empty_sealed_bytes_are_refused_as_unsealed() {
    let (_dir, store) = temp_store(Limits::default());
    assert!(matches!(
        store.save(&secret(b"")),
        Err(ClipError::SecretNotSealed)
    ));
    let id = store.save(&secret(b"sealed")).unwrap();
    assert!(matches!(
        store.update(&Saved { id, ..secret(b"") }),
        Err(ClipError::SecretNotSealed)
    ));
    let stored = store.saved().unwrap().items;
    assert_eq!(stored[0].sealed, Some(Sealed(b"sealed".to_vec())));
}

#[test]
fn a_folder_scope_without_a_path_is_refused_on_save_and_update() {
    let (_dir, store) = temp_store(Limits::default());
    let pathless = Saved {
        scope: folder(""),
        ..item(SavedKind::Path, "p")
    };
    assert!(matches!(
        store.save(&pathless),
        Err(ClipError::Malformed(_))
    ));
    let id = store.save(&item(SavedKind::Path, "p")).unwrap();
    assert!(matches!(
        store.update(&Saved { id, ..pathless }),
        Err(ClipError::Malformed(_))
    ));
    assert_eq!(store.saved().unwrap().items[0].scope, Scope::Global);
}

#[test]
fn over_cap_title_user_and_scope_are_refused_and_at_cap_is_kept() {
    let (_dir, store) = temp_store(small_cap());
    let at_cap = Saved {
        title: "t".repeat(MAX_TITLE_BYTES),
        user: Some("u".repeat(MAX_TITLE_BYTES)),
        scope: folder("/1234567"),
        ..item(SavedKind::Snippet, "body")
    };
    let id = store.save(&at_cap).unwrap();
    let over = [
        Saved {
            title: "t".repeat(MAX_TITLE_BYTES + 1),
            ..at_cap.clone()
        },
        Saved {
            user: Some("u".repeat(MAX_TITLE_BYTES + 1)),
            ..at_cap.clone()
        },
        Saved {
            scope: folder("/12345678"),
            ..at_cap.clone()
        },
    ];
    for item in &over {
        assert!(matches!(store.save(item), Err(ClipError::TooLarge { .. })));
        let edit = Saved { id, ..item.clone() };
        assert!(matches!(
            store.update(&edit),
            Err(ClipError::TooLarge { .. })
        ));
    }
    assert_eq!(store.saved().unwrap().items, vec![Saved { id, ..at_cap }]);
}

#[test]
fn a_full_saved_list_refuses_a_new_item_until_one_is_deleted() {
    let limits = Limits {
        max_saved_rows: 2,
        ..Limits::default()
    };
    let (_dir, store) = temp_store(limits);
    let first = store.save(&item(SavedKind::Snippet, "one")).unwrap();
    store.save(&item(SavedKind::Snippet, "two")).unwrap();
    assert!(matches!(
        store.save(&item(SavedKind::Snippet, "three")),
        Err(ClipError::TooManySaved { cap: 2 })
    ));
    assert_eq!(store.saved().unwrap().items.len(), 2);
    let edit = Saved {
        id: first,
        ..item(SavedKind::Snippet, "edited")
    };
    store.update(&edit).unwrap();
    store.delete_saved(first).unwrap();
    store.save(&item(SavedKind::Snippet, "three")).unwrap();
    assert_eq!(store.saved().unwrap().items.len(), 2);
}
