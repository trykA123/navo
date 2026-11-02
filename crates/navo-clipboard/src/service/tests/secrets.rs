use std::fs;

use super::*;
use crate::model::{Saved, SavedKind, Scope};
use crate::service::secret::wipe_hook;
use crate::service::{FailKind, SECRET_CLEAR_AFTER};

pub(super) const PLAIN: &str = "canary-pass-7781";

pub(super) fn secret_item(title: &str) -> Saved {
    Saved {
        id: 0,
        kind: SavedKind::Secret,
        title: title.to_owned(),
        body: None,
        user: Some("admin".to_owned()),
        sealed: None,
        scope: Scope::Global,
        created_ms: 1,
        updated_ms: 1,
    }
}

pub(super) fn secret(text: &str) -> Option<SecretBytes> {
    Some(SecretBytes::from(text.to_owned()))
}

pub(super) fn save_secret(h: &Harness, text: &str) -> i64 {
    answer(h.service.save(secret_item("Mail"), secret(text))).unwrap()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The bytes of the database and its journal files, as they are on disk.
fn file_bytes(h: &Harness) -> Vec<u8> {
    let mut bytes = Vec::new();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let path = PathBuf::from(format!("{}{suffix}", h.db().display()));
        bytes.extend(fs::read(path).unwrap_or_default());
    }
    bytes
}

fn stored_saved(h: &Harness) -> Vec<Saved> {
    ClipStore::open(&h.db(), Limits::default())
        .unwrap()
        .saved()
        .unwrap()
        .items
}

#[test]
fn only_sealed_bytes_reach_the_database_file() {
    let h = plain();
    save_secret(&h, PLAIN);
    assert_eq!(
        *h.sealer.inputs.lock().unwrap(),
        [PLAIN.as_bytes().to_vec()]
    );
    let before_stop = file_bytes(&h);
    assert!(!contains(&before_stop, PLAIN.as_bytes()));
    assert!(h.service.stop());

    let bytes = file_bytes(&h);
    assert!(!contains(&bytes, PLAIN.as_bytes()));
    assert!(contains(&bytes, &fake_seal(PLAIN.as_bytes())));
    let stored = stored_saved(&h);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].body, None);
    assert_eq!(
        stored[0].sealed.as_ref().unwrap().0,
        fake_seal(PLAIN.as_bytes())
    );
}

#[test]
fn the_snapshot_holds_a_secret_title_and_user_only() {
    let h = plain();
    save_secret(&h, PLAIN);
    let snapshot = wait_for(&h.service, |s| s.saved.len() == 1);
    let item = &snapshot.saved[0];
    assert_eq!(item.title, "Mail");
    assert_eq!(item.user.as_deref(), Some("admin"));
    assert_eq!(item.body, None);
    assert_eq!(item.sealed, None);
    let shown = format!("{snapshot:?} {item:?} {:?}", snapshot.rows().saved);
    assert!(!shown.contains("canary"), "{shown}");
}

#[test]
fn reveal_returns_the_text_and_wipes_it_on_drop() {
    let h = plain();
    let id = save_secret(&h, PLAIN);
    let revealed = answer(h.service.reveal(id)).unwrap();
    assert_eq!(revealed.as_str(), Some(PLAIN));
    assert!(!format!("{revealed:?}").contains("canary"));
    drop(revealed);
    let wiped = wipe_hook::take().unwrap();
    assert!(wiped.len() >= PLAIN.len());
    assert!(wiped.iter().all(|byte| *byte == 0));
}

#[test]
fn reveal_of_a_plain_item_or_an_unknown_id_fails() {
    let h = plain();
    let item = Saved {
        kind: SavedKind::Snippet,
        body: Some("text".into()),
        ..secret_item("Snippet")
    };
    let id = answer(h.service.save(item, None)).unwrap();
    assert!(matches!(
        answer(h.service.reveal(id)),
        Err(FailKind::Refused)
    ));
    assert!(matches!(
        answer(h.service.reveal(999)),
        Err(FailKind::NotFound)
    ));
}

#[test]
fn without_a_sealer_a_secret_is_refused_and_nothing_is_stored() {
    let h = without_sealer();
    let result = answer(h.service.save(secret_item("Mail"), secret(PLAIN)));
    assert_eq!(result, Err(FailKind::SecretsUnsupported));
    assert_eq!(h.service.last_error(), Some(FailKind::SecretsUnsupported));
    let snippet = Saved {
        kind: SavedKind::Snippet,
        body: Some("still works".into()),
        ..secret_item("Snippet")
    };
    answer(h.service.save(snippet, None)).unwrap();
    assert!(h.service.stop());
    let stored = stored_saved(&h);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, SavedKind::Snippet);
    assert!(!contains(&file_bytes(&h), PLAIN.as_bytes()));
}

#[test]
fn a_secret_with_a_plain_body_is_refused_before_sealing() {
    let h = plain();
    let item = Saved {
        body: Some(PLAIN.to_owned()),
        ..secret_item("Mail")
    };
    assert_eq!(
        answer(h.service.save(item, secret(PLAIN))),
        Err(FailKind::Refused)
    );
    assert_eq!(
        answer(h.service.save(secret_item("Mail"), None)),
        Err(FailKind::Refused)
    );
    assert!(h.sealer.inputs.lock().unwrap().is_empty());
    assert!(h.service.stop());
    assert!(stored_saved(&h).is_empty());
    assert!(!contains(&file_bytes(&h), PLAIN.as_bytes()));
}

#[test]
fn a_plain_item_with_secret_text_is_refused() {
    let h = plain();
    let item = Saved {
        kind: SavedKind::Snippet,
        body: Some("text".into()),
        ..secret_item("Snippet")
    };
    assert_eq!(
        answer(h.service.save(item, secret(PLAIN))),
        Err(FailKind::Refused)
    );
    assert!(h.service.stop());
    assert!(stored_saved(&h).is_empty());
    assert!(!contains(&file_bytes(&h), PLAIN.as_bytes()));
}

#[test]
fn an_update_without_new_text_keeps_the_sealed_secret() {
    let h = plain();
    save_secret(&h, PLAIN);
    let shown = wait_for(&h.service, |s| s.saved.len() == 1).saved[0].clone();
    let renamed = Saved {
        title: "Mail (work)".into(),
        ..shown.clone()
    };
    answer(h.service.update(renamed, None)).unwrap();
    let snapshot = wait_for(&h.service, |s| s.saved[0].title == "Mail (work)");
    assert_eq!(snapshot.saved[0].sealed, None);
    let revealed = answer(h.service.reveal(shown.id)).unwrap();
    assert_eq!(revealed.as_str(), Some(PLAIN));

    answer(h.service.update(shown.clone(), secret("canary-new-pass"))).unwrap();
    let revealed = answer(h.service.reveal(shown.id)).unwrap();
    assert_eq!(revealed.as_str(), Some("canary-new-pass"));
}

#[test]
fn a_secret_becomes_another_kind_only_with_new_text() {
    let h = plain();
    let id = save_secret(&h, PLAIN);
    let as_snippet = |body: Option<&str>| Saved {
        id,
        kind: SavedKind::Snippet,
        body: body.map(str::to_owned),
        ..secret_item("Mail")
    };
    for body in [None, Some("")] {
        let refused = answer(h.service.update(as_snippet(body), None));
        assert_eq!(refused, Err(FailKind::Refused));
    }
    let kept = answer(h.service.reveal(id)).unwrap();
    assert_eq!(
        kept.as_str(),
        Some(PLAIN),
        "a refused update keeps the secret"
    );
    drop(kept);

    answer(h.service.update(as_snippet(Some("plain note")), None)).unwrap();
    assert_eq!(answer(h.service.reveal(id)).err(), Some(FailKind::Refused));
    assert!(h.service.stop());
    let stored = stored_saved(&h);
    assert_eq!(stored[0].kind, SavedKind::Snippet);
    assert_eq!(stored[0].sealed, None, "the sealed bytes are gone for good");
    assert!(!contains(&file_bytes(&h), PLAIN.as_bytes()));
}

#[test]
fn a_plain_item_updates_without_a_body() {
    let h = plain();
    let item = Saved {
        kind: SavedKind::Snippet,
        body: Some("one".into()),
        ..secret_item("Snippet")
    };
    let id = answer(h.service.save(item.clone(), None)).unwrap();
    let edited = Saved {
        id,
        body: Some("two".into()),
        ..item
    };
    answer(h.service.update(edited, None)).unwrap();
}

#[test]
fn copy_user_writes_the_user_name_as_a_private_copy() {
    let h = plain();
    let id = save_secret(&h, PLAIN);
    answer(h.service.copy_user(id)).unwrap();
    assert_eq!(h.sink.board().text.as_deref(), Some("admin"));
    assert!(h.sink.calls().contains(&Call::Private("admin".into())));
    h.advance(SECRET_CLEAR_AFTER);
    wait_until(|| h.sink.clears().len() == 1);
}

#[test]
fn copy_user_refuses_an_item_without_a_user_or_a_secret() {
    let h = plain();
    let bare = Saved {
        user: None,
        ..secret_item("Bare")
    };
    let bare = answer(h.service.save(bare, secret(PLAIN))).unwrap();
    assert_eq!(answer(h.service.copy_user(bare)), Err(FailKind::Refused));
    let snippet = Saved {
        kind: SavedKind::Snippet,
        body: Some("text".into()),
        ..secret_item("Snippet")
    };
    let snippet = answer(h.service.save(snippet, None)).unwrap();
    assert_eq!(answer(h.service.copy_user(snippet)), Err(FailKind::Refused));
    assert_eq!(answer(h.service.copy_user(999)), Err(FailKind::NotFound));
    assert!(h.sink.calls().is_empty());
}

#[test]
fn a_secret_in_a_dropped_request_is_wiped() {
    let h = plain();
    assert!(h.service.stop());
    let reply = h.service.save(secret_item("Mail"), secret(PLAIN));
    assert_eq!(answer(reply), Err(FailKind::Busy));
    let wiped = wipe_hook::take().unwrap();
    assert!(wiped.iter().all(|byte| *byte == 0));
}
