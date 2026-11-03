use navo_core::Location;
use navo_ui::clip_palette::{ClipMode, ClipboardHooks, How, Scope};

use super::super::tests::{Rig, on};
use super::{Seen, Spy, finish_all, wait_gone};

const NOTE: &str = "NOTE-4e8d21-canary";
const FOLDER: &str = "FOLDER-b71c05-canary";

/// A note's text may sit in the store while the note lives; it reaches no row, log, report or
/// `Debug` text, and leaves the files with the note. The folder name is a row title by design
/// and is held to the same logs, report and `Debug` rule.
#[test]
fn privacy_notes_leave_no_trace() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::new();
    let mut seen = Seen::default();
    rig.apply(on());
    let here = Location::new(format!("/work/{FOLDER}")).unwrap();
    let text = format!("{NOTE} first line\nsecond line");
    rig.module
        .set_note(Scope::Folder(here.clone()), text)
        .unwrap();
    rig.module
        .set_note(Scope::Everywhere, format!("global {NOTE}"))
        .unwrap();
    let found = rig.module.search(NOTE, ClipMode::Notes, Some(&here));
    assert_eq!(found.rows.len(), 2);
    for row in &found.rows {
        assert!(!row.detail.contains(NOTE) && !row.title.contains(NOTE));
        seen.text("note detail", &row.detail);
    }
    let refused = rig.module.copy(found.rows[0].id, How::Copy).unwrap_err();
    seen.text("copy refusal", &refused);
    seen.service(&rig);
    let missing = rig.module.search("zzz", ClipMode::Notes, None);
    assert!(missing.rows.is_empty());
    for row in &found.rows {
        rig.module.delete(row.id).unwrap();
    }
    wait_gone(&rig, NOTE);
    seen.service(&rig);
    rig.module.stop();
    finish_all(&spy, &seen, &[NOTE, FOLDER]);
}
