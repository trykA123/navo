use gpui_kit::TestAppContext;
use navo_core::Location;

use super::actions::{Action, items};
use super::secret::Facts;
use super::tests::{Call, Fake, Opened, history, open, open_in, row};
use super::{ClipId, ClipKind, ClipMode, ClipPaletteEvent, ClipRow, How, Origin, Place};

fn files_row(id: u64) -> ClipRow {
    row(id, ClipKind::Files, "2 files", Origin::Clipboard)
}

fn with_files() -> Vec<ClipRow> {
    let mut rows = history();
    rows.push(files_row(4));
    rows
}

fn menu_at(palette: &Opened, cx: &mut TestAppContext) -> Option<usize> {
    palette
        .palette
        .read_with(cx, |palette, _| palette.overlay.menu)
}

fn calls(fake: &Fake) -> Vec<Call> {
    std::mem::take(&mut *fake.calls.lock().expect("calls"))
}

#[gpui_kit::test]
fn enter_copies_the_selected_row_and_says_so(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.press("enter", cx);
    assert_eq!(calls(&fake), [Call::Copy(ClipId(1), How::Copy)]);
    assert_eq!(
        palette.label("clip-palette-status", cx).as_deref(),
        Some("Copied")
    );
    assert!(palette.events.borrow().is_empty());
}

#[gpui_kit::test]
fn enter_on_a_path_goes_there_without_copying(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.press("down", cx);
    palette.press("enter", cx);
    let target = Location::new("beta path").expect("location");
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::GoTo(target)]);
    assert!(calls(&fake).is_empty());
}

#[gpui_kit::test]
fn ctrl_c_on_a_path_copies_it_and_stays(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.press("down", cx);
    palette.press("ctrl-c", cx);
    assert_eq!(calls(&fake), [Call::Copy(ClipId(2), How::Copy)]);
    assert!(palette.events.borrow().is_empty());
}

#[gpui_kit::test]
fn ctrl_v_on_files_puts_them_back_and_asks_for_the_paste(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], with_files());
    let palette = open(cx, fake.clone());
    palette.press("ctrl-v", cx);
    assert!(calls(&fake).is_empty());
    for _ in 0..3 {
        palette.press("down", cx);
    }
    palette.press("ctrl-v", cx);
    assert_eq!(calls(&fake), [Call::Copy(ClipId(4), How::Copy)]);
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::PasteIntoPane]);
}

#[gpui_kit::test]
fn delete_removes_the_row_on_the_next_snapshot(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.press("delete", cx);
    assert_eq!(calls(&fake), [Call::Delete(ClipId(1))]);
    assert_eq!(palette.titles(cx), ["beta path", "gamma snippet"]);
    assert_eq!(
        palette.label("clip-palette-status", cx).as_deref(),
        Some("Deleted")
    );
}

#[gpui_kit::test]
fn delete_edits_the_search_text_while_one_is_typed(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.type_text("alpha", cx);
    palette.press("delete", cx);
    assert!(calls(&fake).is_empty());
    assert_eq!(palette.titles(cx), ["alpha note"]);
}

#[gpui_kit::test]
fn ctrl_k_opens_the_menu_and_escape_closes_it_first(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake);
    palette.press("ctrl-k", cx);
    assert_eq!(menu_at(&palette, cx), Some(0));
    assert!(palette.label("clip-palette-menu", cx).is_some());
    palette.press("escape", cx);
    assert_eq!(menu_at(&palette, cx), None);
    assert!(palette.events.borrow().is_empty());
    palette.press("escape", cx);
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::Dismissed]);
}

#[gpui_kit::test]
fn enter_in_the_menu_runs_the_highlighted_action(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open(cx, fake.clone());
    palette.press("ctrl-k", cx);
    palette.press("down", cx);
    palette.press("down", cx);
    palette.press("enter", cx);
    assert_eq!(calls(&fake), [Call::Delete(ClipId(1))]);
    assert_eq!(menu_at(&palette, cx), None);
}

#[test]
fn the_menu_offers_what_the_row_can_do() {
    let keys = |kind| -> Vec<Action> {
        let row = row(1, kind, "x", Origin::Clipboard);
        items(&row, Facts::default())
            .iter()
            .map(|item| item.action)
            .collect()
    };
    assert_eq!(
        keys(ClipKind::Text),
        [Action::Enter, Action::Save, Action::Delete, Action::Manage]
    );
    assert_eq!(
        keys(ClipKind::Path),
        [
            Action::Enter,
            Action::CopyOnly,
            Action::Save,
            Action::Delete,
            Action::Manage
        ]
    );
    assert_eq!(
        keys(ClipKind::Files),
        [
            Action::Enter,
            Action::PasteFiles,
            Action::Save,
            Action::Delete,
            Action::Manage
        ]
    );
    assert_eq!(
        keys(ClipKind::Image),
        [Action::Enter, Action::Delete, Action::Manage]
    );
}

#[gpui_kit::test]
fn from_anywhere_enter_on_a_path_copies_it_and_asks_for_the_paste_back(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    let palette = open_in(cx, fake.clone(), Place::Anywhere);
    palette.press("down", cx);
    palette.press("enter", cx);
    assert_eq!(calls(&fake), [Call::Copy(ClipId(2), How::Paste)]);
    assert_eq!(*palette.events.borrow(), [ClipPaletteEvent::PasteBack]);
}

#[gpui_kit::test]
fn from_anywhere_a_refused_write_says_so_and_asks_for_no_paste(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], history());
    fake.refuse_copy
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let palette = open_in(cx, fake.clone(), Place::Anywhere);
    palette.press("enter", cx);
    assert_eq!(calls(&fake), [Call::Copy(ClipId(1), How::Paste)]);
    assert!(palette.events.borrow().is_empty());
    assert_eq!(
        palette.label("clip-palette-status", cx).as_deref(),
        Some("clipboard busy")
    );
}

#[gpui_kit::test]
fn from_anywhere_ctrl_v_on_files_is_no_pane_paste(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], with_files());
    let palette = open_in(cx, fake.clone(), Place::Anywhere);
    for _ in 0..3 {
        palette.press("down", cx);
    }
    palette.press("ctrl-v", cx);
    assert!(calls(&fake).is_empty());
    assert!(palette.events.borrow().is_empty());
}

#[test]
fn from_anywhere_the_menu_offers_paste_and_copy_only() {
    let anywhere = Facts {
        place: Place::Anywhere,
        ..Facts::default()
    };
    let labels = |row: &ClipRow| -> Vec<String> {
        items(row, anywhere)
            .into_iter()
            .map(|item| item.label)
            .collect()
    };
    let text = row(1, ClipKind::Text, "alpha", Origin::Clipboard);
    assert_eq!(labels(&text)[..2], ["Paste", "Copy only"]);
    let files = files_row(4);
    assert!(
        !items(&files, anywhere)
            .iter()
            .any(|item| item.action == Action::PasteFiles)
    );
    let secret = row(5, ClipKind::Secret, "server", Origin::Saved);
    assert_eq!(labels(&secret)[..2], ["Paste, hidden", "Show"]);
}
