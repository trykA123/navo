use std::sync::Arc;

use gpui_kit::TestAppContext;
use navo_core::Location;

use super::state::{Entry, NotesHead};
use super::tests::{Call, Fake, Opened, open_at, row};
use super::{
    ClipId, ClipKind, ClipMode, ClipPaletteEvent, ClipRow, FolderProbe, Origin, Place, Scope,
};

const HERE: &str = "/work/alpha";
const BETA: &str = "/work/beta";
const TYPED: &str = "typed words";

fn folder(path: &str) -> Location {
    Location::new(path).expect("location")
}

fn note(id: u64, title: &str, scope: Scope) -> ClipRow {
    let mut note = row(id, ClipKind::Note, title, Origin::Saved);
    note.scope = scope;
    note
}

fn notes() -> Vec<ClipRow> {
    vec![
        note(201, "Global note", Scope::Everywhere),
        note(202, "beta", Scope::Folder(folder(BETA))),
        note(203, "alpha", Scope::Folder(folder(HERE))),
    ]
}

fn stored(fake: &Fake) {
    let mut stored = fake.notes.lock().expect("notes");
    stored.push((Scope::Folder(folder(HERE)), "alpha text".into()));
    stored.push((Scope::Folder(folder(BETA)), "beta text".into()));
    stored.push((Scope::Everywhere, "global text".into()));
}

fn open_notes(
    cx: &mut TestAppContext,
    place: Place,
    probe: Option<FolderProbe>,
) -> (Opened, Arc<Fake>) {
    let fake = Fake::new(vec![ClipMode::Notes], notes());
    stored(&fake);
    let here = (place == Place::Window).then(|| folder(HERE));
    (open_at(cx, fake.clone(), (place, here, probe)), fake)
}

fn entries(palette: &Opened, cx: &mut TestAppContext) -> Vec<Entry> {
    palette
        .palette
        .read_with(cx, |palette, _| palette.state.entries().to_vec())
}

fn writes(fake: &Fake) -> Vec<(Scope, String)> {
    fake.note_writes.lock().expect("writes").clone()
}

fn count(palette: &Opened, cx: &mut TestAppContext) -> Option<usize> {
    palette
        .palette
        .read_with(cx, |palette, _| palette.state.count_of(ClipMode::Notes))
}

#[gpui_kit::test]
fn the_folder_note_shows_first_and_the_others_follow_under_their_head(cx: &mut TestAppContext) {
    let (palette, _) = open_notes(cx, Place::Window, None);
    assert_eq!(palette.titles(cx), ["alpha", "Global note", "beta"]);
    assert_eq!(
        entries(&palette, cx),
        [
            Entry::NotesHead(NotesHead::ThisFolder),
            Entry::Row(0),
            Entry::NotesHead(NotesHead::Others),
            Entry::Row(1),
            Entry::Row(2),
        ]
    );
}

#[gpui_kit::test]
fn the_global_note_lists_under_other_notes(cx: &mut TestAppContext) {
    let (palette, _) = open_notes(cx, Place::Window, None);
    let (others, global) = palette.palette.read_with(cx, |palette, _| {
        let entries = palette.state.entries().to_vec();
        let others = entries
            .iter()
            .position(|e| *e == Entry::NotesHead(NotesHead::Others));
        let global = palette
            .state
            .results()
            .rows
            .iter()
            .position(|row| row.scope == Scope::Everywhere)
            .and_then(|at| entries.iter().position(|e| *e == Entry::Row(at)));
        (others, global)
    });
    assert!(others < global && global.is_some(), "{others:?} {global:?}");
}

#[gpui_kit::test]
fn a_folder_without_a_note_gets_an_empty_row_that_is_not_counted(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::Notes], notes()[..2].to_vec());
    let palette = open_at(cx, fake, (Place::Window, Some(folder(HERE)), None));
    assert_eq!(palette.titles(cx), ["alpha", "Global note", "beta"]);
    assert_eq!(count(&palette, cx), Some(2));
    palette.type_text("beta", cx);
    assert_eq!(palette.titles(cx), ["beta"]);
    assert_eq!(entries(&palette, cx), [Entry::Row(0)]);
}

#[gpui_kit::test]
fn from_anywhere_the_notes_are_plain_rows(cx: &mut TestAppContext) {
    let (palette, _) = open_notes(cx, Place::Anywhere, None);
    assert_eq!(palette.titles(cx), ["Global note", "beta", "alpha"]);
    assert_eq!(
        entries(&palette, cx),
        [Entry::Row(0), Entry::Row(1), Entry::Row(2)]
    );
}

#[gpui_kit::test]
fn typing_saves_once_after_the_pause(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    palette.type_text("X", cx);
    palette.wait(300, cx);
    palette.type_text("Y", cx);
    palette.wait(300, cx);
    assert!(writes(&fake).is_empty());
    palette.wait(250, cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, Scope::Folder(folder(HERE)));
    assert!(saved[0].1.contains("XY") && saved[0].1.contains("alpha text"));
    palette.wait(2_000, cx);
    assert_eq!(writes(&fake).len(), 1);
}

#[gpui_kit::test]
fn closing_saves_what_is_still_waiting(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    palette.type_text(TYPED, cx);
    assert!(writes(&fake).is_empty());
    palette.close(cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].1.contains(TYPED));
}

#[gpui_kit::test]
fn ctrl_e_focuses_the_editor_and_escape_leaves_it_before_closing(cx: &mut TestAppContext) {
    let (palette, _) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    palette.type_text("N", cx);
    assert_eq!(palette.query(cx), "");
    palette.press("escape", cx);
    assert!(palette.events.borrow().is_empty());
    palette.type_text("q", cx);
    assert_eq!(palette.query(cx), "q");
    palette.press("escape", cx);
    assert_eq!(palette.query(cx), "");
    palette.press("escape", cx);
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::Dismissed]);
}

#[gpui_kit::test]
fn enter_on_a_note_opens_the_editor(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("enter", cx);
    palette.type_text("E", cx);
    assert_eq!(palette.query(cx), "");
    palette.wait(600, cx);
    assert_eq!(writes(&fake).len(), 1);
}

#[gpui_kit::test]
fn the_editor_takes_enter_and_the_arrows_while_it_has_focus(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    palette.press("down", cx);
    palette.press("enter", cx);
    let selected = palette
        .palette
        .read_with(cx, |palette, _| palette.state.selected());
    assert_eq!(selected, 0);
    palette.wait(600, cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].1.contains('\n'));
}

#[gpui_kit::test]
fn moving_to_another_note_saves_the_edit_first(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    palette.type_text("X", cx);
    palette.press("escape", cx);
    palette.press("down", cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, Scope::Folder(folder(HERE)));
    palette.press("ctrl-e", cx);
    palette.type_text("Z", cx);
    palette.wait(600, cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[1].0, Scope::Everywhere);
    assert!(saved[1].1.contains("global text"));
}

#[gpui_kit::test]
fn a_missing_folder_is_tagged_and_its_note_is_kept(cx: &mut TestAppContext) {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = asked.clone();
    let probe: FolderProbe = Arc::new(move |at| {
        log.lock().expect("asked").push(at.as_str().to_owned());
        at.as_str() == BETA
    });
    let (palette, fake) = open_notes(cx, Place::Window, Some(probe));
    let missing = palette
        .palette
        .read_with(cx, |palette, _| palette.missing_folders());
    assert!(missing.contains(&folder(BETA)) && missing.len() == 1);
    assert!(!asked.lock().expect("asked").iter().any(|at| at == HERE));
    palette.press("down", cx);
    palette.press("down", cx);
    palette.press("ctrl-e", cx);
    palette.type_text("Z", cx);
    palette.wait(600, cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, Scope::Folder(folder(BETA)));
    assert!(saved[0].1.contains("beta text"));
}

#[gpui_kit::test]
fn a_note_that_could_not_be_read_is_never_overwritten(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::Notes], notes());
    let palette = open_at(cx, fake.clone(), (Place::Window, Some(folder(HERE)), None));
    palette.press("ctrl-e", cx);
    palette.type_text("lost", cx);
    palette.wait(600, cx);
    assert!(writes(&fake).is_empty());
}

#[gpui_kit::test]
fn from_anywhere_the_editor_opens_the_global_note(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Anywhere, None);
    palette.press("ctrl-e", cx);
    palette.type_text("G", cx);
    palette.wait(600, cx);
    let saved = writes(&fake);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, Scope::Everywhere);
    assert!(saved[0].1.contains("global text"));
}

#[gpui_kit::test]
fn deleting_a_note_row_drops_the_edits_still_waiting(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    palette.type_text("X", cx);
    palette.press("escape", cx);
    palette.press("delete", cx);
    palette.wait(600, cx);
    assert_eq!(
        *fake.calls.lock().expect("calls"),
        [Call::Delete(ClipId(203))]
    );
    assert!(writes(&fake).is_empty());
}

#[gpui_kit::test]
fn the_placeholder_of_an_empty_folder_note_is_not_deleted(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::Notes], notes()[..1].to_vec());
    let palette = open_at(cx, fake.clone(), (Place::Window, Some(folder(HERE)), None));
    palette.press("delete", cx);
    assert!(fake.calls.lock().expect("calls").is_empty());
}

fn held_text(palette: &Opened, cx: &mut TestAppContext) -> String {
    palette.palette.read_with(cx, |palette, cx| {
        palette.note.editor.read(cx).value().to_string()
    })
}

#[gpui_kit::test]
fn deleting_the_folder_note_in_the_library_empties_the_editor(cx: &mut TestAppContext) {
    let (palette, fake) = open_notes(cx, Place::Window, None);
    palette.press("ctrl-e", cx);
    assert!(held_text(&palette, cx).contains("alpha text"));
    palette.press("escape", cx);
    // Opening the library frees the editor at the next frame; the delete must not rely on it.
    palette
        .palette
        .update(cx, |palette, cx| palette.open_manage(cx));
    cx.run_until_parked();
    palette.palette.update(cx, |palette, cx| {
        palette.manage_step(2, cx);
        palette.manage_delete(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        *fake.calls.lock().expect("calls"),
        [Call::Delete(ClipId(203))]
    );
    palette.press("escape", cx);
    assert_eq!(held_text(&palette, cx), "");
    palette.wait(600, cx);
    assert!(writes(&fake).is_empty());
}
