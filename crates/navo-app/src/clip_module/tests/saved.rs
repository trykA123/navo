use navo_core::Location;
use navo_ui::clip_palette::{
    ClipId, ClipKind, ClipMode, ClipboardHooks, How, SavedDraft, Scope, SecretText,
};

use super::{Rig, XorSealer, on, wait_until_len};

use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

fn draft(kind: ClipKind, title: &str, body: &str) -> SavedDraft {
    SavedDraft {
        kind,
        title: title.to_owned(),
        body: SecretText::new(body.to_owned()),
        user: None,
        scope: Scope::Everywhere,
    }
}

fn secrets_rig() -> Rig {
    let rig = Rig::with_sealer(Arc::new(XorSealer));
    rig.apply(on());
    rig
}

fn rows(rig: &Rig, mode: ClipMode, query: &str) -> Vec<(ClipId, String)> {
    let found = rig.module.search(query, mode, None);
    found.rows.into_iter().map(|r| (r.id, r.title)).collect()
}

fn wait_rows(rig: &Rig, mode: ClipMode, len: usize) -> Vec<(ClipId, String)> {
    for _ in 0..1000 {
        let found = rows(rig, mode, "");
        if found.len() == len {
            return found;
        }
        sleep(Duration::from_millis(5));
    }
    panic!("{mode:?} never reached {len} rows");
}

fn folder(path: &std::path::Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn a_saved_snippet_is_listed_copied_and_deleted() {
    let rig = secrets_rig();
    let id = rig
        .module
        .save(draft(ClipKind::Text, "greeting", "hello there"))
        .unwrap();
    let found = wait_rows(&rig, ClipMode::Saved, 1);
    assert_eq!(found, [(id, "greeting".to_owned())]);
    let row = &rig.module.search("", ClipMode::Saved, None).rows[0];
    assert_eq!(row.detail, "hello there");
    assert!(wait_rows(&rig, ClipMode::Secrets, 0).is_empty());
    rig.module.copy(id, How::Copy).unwrap();
    for _ in 0..1000 {
        if !rig.sink.texts.lock().unwrap().is_empty() {
            break;
        }
        sleep(Duration::from_millis(5));
    }
    assert_eq!(*rig.sink.texts.lock().unwrap(), ["hello there"]);
    rig.module.delete(id).unwrap();
    wait_rows(&rig, ClipMode::Saved, 0);
}

#[test]
fn saved_and_history_ids_never_meet() {
    let rig = secrets_rig();
    rig.emit("copied text");
    wait_until_len(&rig, 1);
    let saved = rig
        .module
        .save(draft(ClipKind::Text, "kept", "kept text"))
        .unwrap();
    wait_rows(&rig, ClipMode::Saved, 1);
    let ids: Vec<_> = rows(&rig, ClipMode::All, "")
        .into_iter()
        .map(|r| r.0)
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&saved));
    assert_ne!(ids[0], ids[1]);
    rig.module.delete(saved).unwrap();
    wait_rows(&rig, ClipMode::Saved, 0);
    assert_eq!(rows(&rig, ClipMode::Recent, "").len(), 1, "history stays");
}

#[test]
fn a_history_row_saves_as_a_snippet_and_stays() {
    let rig = secrets_rig();
    rig.emit("keep this line\nsecond");
    wait_until_len(&rig, 1);
    let history = rows(&rig, ClipMode::Recent, "")[0].0;
    let saved = rig.module.save_from_history(history).unwrap();
    let found = wait_rows(&rig, ClipMode::Saved, 1);
    assert_eq!(found, [(saved, "keep this line".to_owned())]);
    assert_eq!(rows(&rig, ClipMode::Recent, "").len(), 1);
    assert!(rig.module.save_from_history(saved).is_err());
}

#[test]
fn a_secret_is_listed_by_title_revealed_and_copied_privately() {
    let rig = secrets_rig();
    let id = rig
        .module
        .save(draft(ClipKind::Secret, "bank login", "hunter2"))
        .unwrap();
    assert_eq!(
        wait_rows(&rig, ClipMode::Secrets, 1),
        [(id, "bank login".to_owned())]
    );
    assert!(rows(&rig, ClipMode::Saved, "").is_empty());
    let row = &rig.module.search("", ClipMode::Secrets, None).rows[0];
    assert_eq!(row.kind, ClipKind::Secret);
    assert!(!row.detail.contains("hunter2"));
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "hunter2");
    rig.module.copy(id, How::Copy).unwrap();
    for _ in 0..1000 {
        if !rig.sink.privates.lock().unwrap().is_empty() {
            break;
        }
        sleep(Duration::from_millis(5));
    }
    assert_eq!(*rig.sink.privates.lock().unwrap(), ["hunter2"]);
    assert!(rig.sink.texts.lock().unwrap().is_empty());
    assert!(
        rig.module.reveal(ClipId(1)).is_err(),
        "a history id is no secret"
    );
}

#[test]
fn secrets_need_windows_without_a_sealer() {
    let rig = Rig::new();
    rig.apply(on());
    let need = navo_l10n::tr("clipboard-secrets-need-windows");
    let found = rig.module.search("", ClipMode::Secrets, None);
    assert_eq!(found.notice, Some(need.clone()));
    assert!(
        rig.module
            .search("", ClipMode::Saved, None)
            .notice
            .is_none()
    );
    let saved = rig.module.save(draft(ClipKind::Secret, "bank", "hunter2"));
    assert_eq!(saved, Err(need));
    assert!(rows(&rig, ClipMode::Secrets, "").is_empty());
    assert!(rig.module.save(draft(ClipKind::Text, "ok", "fine")).is_ok());
}

#[test]
fn drafts_the_store_cannot_keep_are_refused() {
    let rig = secrets_rig();
    let refused = navo_l10n::tr("clipboard-refused");
    for kind in [ClipKind::Files, ClipKind::Image] {
        assert_eq!(rig.module.save(draft(kind, "t", "b")), Err(refused.clone()));
    }
    assert_eq!(
        rig.module.save(draft(ClipKind::Text, "  ", "b")),
        Err(refused)
    );
    let long = "x".repeat(2000);
    let too_large = rig.module.save(draft(ClipKind::Text, &long, "b"));
    assert_eq!(too_large, Err(navo_l10n::tr("clipboard-too-large")));
}

#[test]
fn a_folder_note_is_set_read_and_removed() {
    let rig = secrets_rig();
    let dir = tempfile::tempdir().unwrap();
    let here = folder(dir.path());
    assert_eq!(rig.module.note_for(&here), None);
    rig.module
        .set_note(Scope::Folder(here.clone()), "build with care".into())
        .unwrap();
    assert_eq!(
        rig.module.note_for(&here).as_deref(),
        Some("build with care")
    );
    rig.module
        .set_note(Scope::Everywhere, "global".into())
        .unwrap();
    rig.module
        .set_note(Scope::Folder(here.clone()), String::new())
        .unwrap();
    assert_eq!(rig.module.note_for(&here), None);
}

#[test]
fn saved_methods_refuse_while_the_module_is_off() {
    let rig = Rig::new();
    let off = navo_l10n::tr("clipboard-off");
    let item = draft(ClipKind::Text, "t", "b");
    assert_eq!(rig.module.save(item), Err(off.clone()));
    assert_eq!(rig.module.copy_user(ClipId(1)), Err(off.clone()));
    assert_eq!(rig.module.set_note(Scope::Everywhere, "x".into()), Err(off));
    assert_eq!(
        rig.module.reveal(ClipId(1)).err(),
        Some(navo_l10n::tr("clipboard-off"))
    );
}

#[test]
fn a_failed_saved_copy_is_reported_not_swallowed() {
    let rig = secrets_rig();
    let id = rig
        .module
        .save(draft(ClipKind::Text, "gone soon", "text"))
        .unwrap();
    wait_rows(&rig, ClipMode::Saved, 1);
    rig.module.delete(id).unwrap();
    wait_rows(&rig, ClipMode::Saved, 0);
    assert!(rig.module.copy(id, How::Copy).is_err());
    assert!(rig.module.copy_user(id).is_err());
}

#[test]
fn a_saved_item_or_secret_that_cannot_be_deleted_reads_not_deleted() {
    let rig = secrets_rig();
    let plain = rig
        .module
        .save(draft(ClipKind::Text, "kept", "kept text"))
        .unwrap();
    let sealed = rig
        .module
        .save(draft(ClipKind::Secret, "bank", "hunter2"))
        .unwrap();
    rig.refuse_deletes("saved");
    let not_deleted = Err(navo_l10n::tr("clipboard-not-deleted"));
    assert_eq!(rig.module.delete(plain), not_deleted);
    assert_eq!(rig.module.delete(sealed), not_deleted);
    assert_eq!(wait_rows(&rig, ClipMode::Saved, 1)[0].0, plain);
    assert_eq!(wait_rows(&rig, ClipMode::Secrets, 1)[0].0, sealed);
}

#[test]
fn a_secret_whose_log_stays_busy_reads_not_deleted_not_busy() {
    let rig = secrets_rig();
    let sealed = rig
        .module
        .save(draft(ClipKind::Secret, "bank", "hunter2"))
        .unwrap();
    let reader = rig.pin_reader();
    rig.module
        .save(draft(ClipKind::Text, "later", "later text"))
        .unwrap();
    let result = rig.module.delete(sealed);
    drop(reader);
    assert_eq!(result, Err(navo_l10n::tr("clipboard-not-deleted")));
}
