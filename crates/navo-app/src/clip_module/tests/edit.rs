//! Saving a user name, loading an item for edit, updating it and re-pointing a note.

use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use navo_core::Location;
use navo_ui::clip_palette::{
    ClipId, ClipKind, ClipMode, ClipboardHooks, SavedDraft, SavedEdit, Scope, SecretText,
};

use super::{Rig, XorSealer, on};

fn draft(kind: ClipKind, title: &str, body: &str, user: Option<&str>) -> SavedDraft {
    SavedDraft {
        kind,
        title: title.to_owned(),
        body: SecretText::new(body.to_owned()),
        user: user.map(str::to_owned),
        scope: Scope::Everywhere,
    }
}

fn secrets_rig() -> Rig {
    let rig = Rig::with_sealer(Arc::new(XorSealer));
    rig.apply(on());
    rig
}

fn secret(rig: &Rig) -> ClipId {
    let item = draft(ClipKind::Secret, "bank", "hunter2", Some("alice"));
    rig.module.save(item).unwrap()
}

fn refused() -> String {
    navo_l10n::tr("clipboard-refused")
}

fn edit_of(rig: &Rig, id: ClipId) -> SavedEdit {
    rig.module.saved_for_edit(id).unwrap()
}

fn folder(path: &std::path::Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

fn wait_title(rig: &Rig, mode: ClipMode, title: &str) {
    for _ in 0..1000 {
        let found = rig.module.search("", mode, None);
        if found.rows.iter().any(|row| row.title == title) {
            return;
        }
        sleep(Duration::from_millis(5));
    }
    panic!("no {mode:?} row titled {title}");
}

#[test]
fn a_secret_saves_with_its_user_name() {
    let rig = secrets_rig();
    let id = secret(&rig);
    let row = &rig.module.search("", ClipMode::Secrets, None).rows[0];
    assert_eq!(row.detail, "alice");
    let edit = edit_of(&rig, id);
    assert_eq!(edit.user.as_deref(), Some("alice"));
    rig.module.copy_user(id).unwrap();
    assert_eq!(*rig.sink.privates.lock().unwrap(), ["alice"]);
}

#[test]
fn a_user_name_on_a_plain_kind_is_dropped_and_a_blank_one_is_none() {
    let rig = secrets_rig();
    let snippet = draft(ClipKind::Text, "note", "text", Some("ignored"));
    let id = rig.module.save(snippet).unwrap();
    assert_eq!(edit_of(&rig, id).user, None);
    let blank = draft(ClipKind::Secret, "blank", "pw", Some("  "));
    let id = rig.module.save(blank).unwrap();
    assert_eq!(edit_of(&rig, id).user, None);
}

#[test]
fn loading_for_edit_never_returns_a_secrets_password() {
    let rig = secrets_rig();
    let id = secret(&rig);
    let edit = edit_of(&rig, id);
    assert_eq!(edit.kind, ClipKind::Secret);
    assert_eq!(edit.title, "bank");
    assert_eq!(edit.body.expose(), "");
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "hunter2");
}

#[test]
fn loading_a_snippet_returns_title_body_and_scope() {
    let rig = secrets_rig();
    let dir = tempfile::tempdir().unwrap();
    let mut item = draft(ClipKind::Text, "greeting", "hello", None);
    item.scope = Scope::Folder(folder(dir.path()));
    let id = rig.module.save(item).unwrap();
    let edit = edit_of(&rig, id);
    assert_eq!(edit.kind, ClipKind::Text);
    assert_eq!(
        (edit.title.as_str(), edit.body.expose()),
        ("greeting", "hello")
    );
    assert_eq!(edit.scope, Scope::Folder(folder(dir.path())));
    assert!(
        rig.module.saved_for_edit(ClipId(1)).is_err(),
        "a history id"
    );
    assert!(rig.module.saved_for_edit(ClipId(1 << 63 | 99)).is_err());
}

#[test]
fn a_snippet_edit_changes_title_body_and_scope() {
    let rig = secrets_rig();
    let dir = tempfile::tempdir().unwrap();
    let id = rig
        .module
        .save(draft(ClipKind::Text, "old", "old body", None))
        .unwrap();
    let mut item = draft(ClipKind::Text, " new ", "new body", None);
    item.scope = Scope::Folder(folder(dir.path()));
    rig.module.update_saved(id, item).unwrap();
    let edit = edit_of(&rig, id);
    assert_eq!(
        (edit.title.as_str(), edit.body.expose()),
        ("new", "new body")
    );
    assert_eq!(edit.scope, Scope::Folder(folder(dir.path())));
    let blank = draft(ClipKind::Text, "  ", "x", None);
    assert_eq!(rig.module.update_saved(id, blank), Err(refused()));
    assert_eq!(edit_of(&rig, id).title, "new");
}

#[test]
fn a_secret_edit_with_an_empty_body_keeps_the_password() {
    let rig = secrets_rig();
    let id = secret(&rig);
    let item = draft(ClipKind::Secret, "bank 2", "", Some("bob"));
    rig.module.update_saved(id, item).unwrap();
    let edit = edit_of(&rig, id);
    assert_eq!(
        (edit.title.as_str(), edit.user.as_deref()),
        ("bank 2", Some("bob"))
    );
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "hunter2");
    let cleared = draft(ClipKind::Secret, "bank 2", "", None);
    rig.module.update_saved(id, cleared).unwrap();
    assert_eq!(edit_of(&rig, id).user, None);
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "hunter2");
}

#[test]
fn a_secret_edit_with_a_body_replaces_the_password() {
    let rig = secrets_rig();
    let id = secret(&rig);
    let item = draft(ClipKind::Secret, "bank", "n3w-pass", Some("alice"));
    rig.module.update_saved(id, item).unwrap();
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "n3w-pass");
}

#[test]
fn a_secret_changes_kind_only_with_a_new_body() {
    let rig = secrets_rig();
    let id = secret(&rig);
    let bare = draft(ClipKind::Text, "bank", "", None);
    assert_eq!(rig.module.update_saved(id, bare), Err(refused()));
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "hunter2");
    assert_eq!(edit_of(&rig, id).kind, ClipKind::Secret);
    let full = draft(ClipKind::Text, "bank", "plain now", Some("alice"));
    rig.module.update_saved(id, full).unwrap();
    wait_title(&rig, ClipMode::Saved, "bank");
    let edit = edit_of(&rig, id);
    assert_eq!(
        (edit.kind, edit.body.expose()),
        (ClipKind::Text, "plain now")
    );
    assert_eq!(edit.user, None, "a plain item has no user name");
    assert!(rig.module.reveal(id).is_err(), "the sealed bytes are gone");
}

#[test]
fn a_plain_item_becomes_a_secret_only_with_a_body() {
    let rig = secrets_rig();
    let id = rig
        .module
        .save(draft(ClipKind::Text, "login", "text", None))
        .unwrap();
    let bare = draft(ClipKind::Secret, "login", "", Some("al"));
    assert_eq!(rig.module.update_saved(id, bare), Err(refused()));
    let full = draft(ClipKind::Secret, "login", "pw", Some("al"));
    rig.module.update_saved(id, full).unwrap();
    assert_eq!(rig.module.reveal(id).unwrap().expose(), "pw");
    assert_eq!(edit_of(&rig, id).user.as_deref(), Some("al"));
}

#[test]
fn updates_of_other_ids_and_while_off_are_refused() {
    let rig = secrets_rig();
    let item = || draft(ClipKind::Text, "t", "b", None);
    assert_eq!(rig.module.update_saved(ClipId(1), item()), Err(refused()));
    let gone = ClipId(1 << 63 | 99);
    let missing = rig.module.update_saved(gone, item());
    assert_eq!(missing, Err(navo_l10n::tr("clipboard-not-found")));
    let off = Rig::new();
    let message = navo_l10n::tr("clipboard-off");
    assert_eq!(off.module.update_saved(gone, item()), Err(message.clone()));
    assert_eq!(off.module.saved_for_edit(gone).err(), Some(message.clone()));
    let (a, b) = (Scope::Everywhere, Scope::Everywhere);
    assert_eq!(off.module.rename_note(a, b), Err(message));
}

#[test]
fn a_note_moves_to_another_folder() {
    let rig = secrets_rig();
    let (from, to) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (from, to) = (folder(from.path()), folder(to.path()));
    let set = |scope, text: &str| rig.module.set_note(scope, text.to_owned()).unwrap();
    set(Scope::Folder(from.clone()), "kept text");
    let moved = rig
        .module
        .rename_note(Scope::Folder(from.clone()), Scope::Folder(to.clone()));
    assert_eq!(moved, Ok(()));
    assert_eq!(rig.module.note_for(&to).as_deref(), Some("kept text"));
    assert_eq!(rig.module.note_for(&from), None);
}

#[test]
fn a_note_move_refuses_a_taken_target_a_missing_source_and_the_global_note() {
    let rig = secrets_rig();
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (folder(a.path()), folder(b.path()));
    rig.module
        .set_note(Scope::Folder(a.clone()), "a".into())
        .unwrap();
    rig.module
        .set_note(Scope::Folder(b.clone()), "b".into())
        .unwrap();
    let (fa, fb) = (Scope::Folder(a.clone()), Scope::Folder(b.clone()));
    assert!(rig.module.rename_note(fa.clone(), fb.clone()).is_err());
    assert_eq!(rig.module.note_for(&a).as_deref(), Some("a"));
    assert_eq!(rig.module.note_for(&b).as_deref(), Some("b"));
    rig.module.set_note(fa.clone(), String::new()).unwrap();
    let missing = rig.module.rename_note(fa, fb.clone());
    assert_eq!(missing, Err(navo_l10n::tr("clipboard-not-found")));
    assert_eq!(
        rig.module.rename_note(Scope::Everywhere, fb),
        Err(refused())
    );
}
