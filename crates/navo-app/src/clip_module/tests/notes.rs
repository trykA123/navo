use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use navo_core::Location;
use navo_ui::clip_palette::{
    ClipId, ClipKind, ClipMode, ClipRow, ClipboardHooks, How, Origin, SavedDraft, Scope, SecretText,
};

use super::{Rig, XorSealer, on};

const TEXT: &str = "NOTE-TEXT-5d1e7a first line\nsecond line";

fn rig() -> Rig {
    let rig = Rig::with_sealer(Arc::new(XorSealer));
    rig.apply(on());
    rig
}

fn folder(path: &str) -> Location {
    Location::new(path.to_owned()).unwrap()
}

fn set(rig: &Rig, scope: Scope, text: &str) {
    rig.module.set_note(scope, text.to_owned()).unwrap();
}

fn notes(rig: &Rig, query: &str, here: Option<&Location>) -> Vec<ClipRow> {
    rig.module.search(query, ClipMode::Notes, here).rows
}

fn titles(rows: &[ClipRow]) -> Vec<&str> {
    rows.iter().map(|row| row.title.as_str()).collect()
}

fn wait_notes(rig: &Rig, len: usize) {
    for _ in 0..1000 {
        if notes(rig, "", None).len() == len {
            return;
        }
        sleep(Duration::from_millis(5));
    }
    panic!("Notes never reached {len} rows");
}

fn three(rig: &Rig) -> (Location, Location) {
    let (alpha, beta) = (folder("/work/alpha"), folder("/work/beta"));
    set(rig, Scope::Everywhere, "global words");
    set(rig, Scope::Folder(alpha.clone()), TEXT);
    set(rig, Scope::Folder(beta.clone()), "beta words\nmore");
    wait_notes(rig, 3);
    (alpha, beta)
}

#[test]
fn notes_mode_is_offered_on_a_live_module_only() {
    let rig = Rig::new();
    assert!(!rig.module.modes().contains(&ClipMode::Notes));
    rig.apply(on());
    assert_eq!(rig.module.modes().last(), Some(&ClipMode::Notes));
}

#[test]
fn every_note_is_one_row_and_the_active_folder_comes_first() {
    let rig = rig();
    let (_, beta) = three(&rig);
    let rows = notes(&rig, "", None);
    assert_eq!(titles(&rows), ["Global note", "alpha", "beta"]);
    assert!(rows.iter().all(|row| row.kind == ClipKind::Note));
    assert!(rows.iter().all(|row| row.from == Origin::Saved));
    assert_eq!(rows[0].scope, Scope::Everywhere);
    assert_eq!(rows[1].scope, Scope::Folder(folder("/work/alpha")));
    let here = notes(&rig, "", Some(&beta));
    assert_eq!(titles(&here), ["beta", "Global note", "alpha"]);
}

#[test]
fn a_row_shows_counts_and_never_the_note_text() {
    let rig = rig();
    three(&rig);
    let rows = notes(&rig, "", None);
    assert_eq!(rows[1].detail, "2 lines · 39 characters");
    for row in notes(&rig, "NOTE-TEXT", None) {
        let fields = [
            &row.title,
            &row.detail,
            row.source_app.as_deref().unwrap_or(""),
        ];
        assert!(fields.iter().all(|field| !field.contains("NOTE-TEXT")));
    }
    let other = rig.module.search("NOTE-TEXT", ClipMode::All, None);
    assert!(other.rows.is_empty());
}

#[test]
fn search_matches_the_folder_name_and_the_text() {
    let rig = rig();
    three(&rig);
    let by_name = notes(&rig, "ALPH", None);
    assert_eq!(titles(&by_name), ["alpha"]);
    let span: Vec<_> = by_name[0].ranges.iter().map(|r| (r.start, r.end)).collect();
    assert_eq!(span, [(0, 4)]);
    let by_text = notes(&rig, "note-text", None);
    assert_eq!(titles(&by_text), ["alpha"]);
    assert!(by_text[0].ranges.is_empty());
    assert_eq!(titles(&notes(&rig, "global", None)), ["Global note"]);
    assert!(notes(&rig, "absent", None).is_empty());
}

#[test]
fn note_ids_never_meet_saved_or_history_ids() {
    let rig = rig();
    three(&rig);
    let saved = rig
        .module
        .save(SavedDraft {
            kind: ClipKind::Text,
            title: "snippet".into(),
            body: SecretText::new("body".into()),
            user: None,
            scope: Scope::Everywhere,
        })
        .unwrap();
    assert!(rig.emit("history text"));
    rig.wait_for_title("history text");
    let history = rig.module.search("", ClipMode::Recent, None).rows[0].id;
    let ids: Vec<ClipId> = notes(&rig, "", None).iter().map(|row| row.id).collect();
    assert_eq!(ids.len(), 3);
    for id in &ids {
        assert_ne!(*id, saved);
        assert_ne!(*id, history);
    }
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    let saved_rows = rig.module.search("", ClipMode::Saved, None).rows;
    assert!(saved_rows.iter().all(|row| !ids.contains(&row.id)));
}

#[test]
fn deleting_a_note_row_removes_the_note() {
    let rig = rig();
    let (alpha, _) = three(&rig);
    let id = notes(&rig, "alpha", None)[0].id;
    rig.module.delete(id).unwrap();
    assert_eq!(titles(&notes(&rig, "", None)), ["Global note", "beta"]);
    assert_eq!(rig.module.note_for(&alpha), None);
    assert_eq!(rig.module.counts().unwrap().notes, 2);
    assert_eq!(
        rig.module.delete(id),
        Err(navo_l10n::tr("clipboard-not-found"))
    );
}

#[test]
fn the_global_note_reads_back_and_empty_after_deleting_it() {
    let rig = rig();
    assert_eq!(rig.module.global_note(), None);
    three(&rig);
    assert_eq!(rig.module.global_note().as_deref(), Some("global words"));
    let id = notes(&rig, "global", None)[0].id;
    rig.module.delete(id).unwrap();
    assert_eq!(rig.module.global_note(), None);
}

#[test]
fn a_note_row_is_not_copied() {
    let rig = rig();
    three(&rig);
    let id = notes(&rig, "global", None)[0].id;
    let refused = Err(navo_l10n::tr("clipboard-refused"));
    assert_eq!(rig.module.copy(id, How::Copy), refused);
    assert_eq!(rig.module.copy(id, How::Paste), refused);
    assert_eq!(rig.module.copy_user(id), refused);
    assert!(rig.sink.texts.lock().unwrap().is_empty());
    assert!(rig.sink.privates.lock().unwrap().is_empty());
}

#[test]
fn counts_include_the_notes() {
    let rig = rig();
    assert_eq!(rig.module.counts().unwrap().notes, 0);
    three(&rig);
    assert_eq!(rig.module.counts().unwrap().notes, 3);
}

#[test]
fn a_note_that_cannot_be_deleted_reads_not_deleted() {
    let rig = rig();
    let (alpha, _) = three(&rig);
    let id = notes(&rig, "alpha", None)[0].id;
    rig.refuse_deletes("notes");
    assert_eq!(
        rig.module.delete(id),
        Err(navo_l10n::tr("clipboard-not-deleted"))
    );
    assert!(rig.module.note_for(&alpha).is_some());
}
