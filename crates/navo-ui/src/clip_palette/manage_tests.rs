use gpui_kit::TestAppContext;

use super::manage_kinds::{KINDS, Kind};
use super::tests::{Call, Fake, Opened, SECRET, open, row};
use super::{ClipId, ClipKind, ClipMode, ClipRow, Origin};

const LIBRARY_MODES: [ClipMode; 4] = [
    ClipMode::All,
    ClipMode::Recent,
    ClipMode::Saved,
    ClipMode::Secrets,
];

fn library() -> Vec<ClipRow> {
    let mut login = row(5, ClipKind::Secret, "epsilon login", Origin::Saved);
    login.detail = SECRET.into();
    vec![
        row(1, ClipKind::Text, "alpha note", Origin::Clipboard),
        row(2, ClipKind::Path, "beta path", Origin::Clipboard),
        row(3, ClipKind::Text, "gamma snippet", Origin::Saved),
        row(4, ClipKind::Path, "delta folder", Origin::Saved),
        login,
        row(6, ClipKind::Text, "zeta snippet", Origin::Saved),
    ]
}

fn opened(cx: &mut TestAppContext) -> (Opened, std::sync::Arc<Fake>) {
    let fake = Fake::new(LIBRARY_MODES.to_vec(), library());
    (open(cx, fake.clone()), fake)
}

fn titles(palette: &Opened, cx: &mut TestAppContext) -> Vec<String> {
    palette.palette.read_with(cx, |palette, _| {
        let rows = palette.manage.as_ref().map(|m| m.rows().clone());
        rows.into_iter()
            .flat_map(|rows| rows.iter().map(|row| row.title.clone()).collect::<Vec<_>>())
            .collect()
    })
}

fn kind(palette: &Opened, cx: &mut TestAppContext) -> Option<Kind> {
    palette
        .palette
        .read_with(cx, |palette, _| palette.manage.as_ref().map(|m| m.kind()))
}

fn counts(palette: &Opened, cx: &mut TestAppContext) -> Vec<Option<usize>> {
    palette.palette.read_with(cx, |palette, _| {
        let manage = palette.manage.as_ref().expect("the library is open");
        KINDS.map(|kind| manage.count_of(kind)).to_vec()
    })
}

#[gpui_kit::test]
fn each_kind_counts_its_rows_and_unlisted_kinds_have_none(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.press("f2", cx);
    assert_eq!(
        counts(&palette, cx),
        [Some(2), Some(2), None, Some(1), Some(1), None]
    );
}

#[gpui_kit::test]
fn counts_follow_the_search(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.type_text("a", cx);
    palette.press("f2", cx);
    assert_eq!(
        counts(&palette, cx),
        [Some(2), Some(2), None, Some(1), Some(0), None]
    );
}

#[gpui_kit::test]
fn f2_on_a_history_row_opens_the_library_on_history(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    assert_eq!(kind(&palette, cx), None);
    palette.press("f2", cx);
    assert_eq!(kind(&palette, cx), Some(Kind::History));
    assert_eq!(titles(&palette, cx), ["alpha note", "beta path"]);
}

#[gpui_kit::test]
fn f2_on_a_saved_row_leaves_it_for_the_edit_form(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.type_text("gamma", cx);
    palette.press("f2", cx);
    assert_eq!(kind(&palette, cx), None);
}

#[gpui_kit::test]
fn the_actions_menu_offers_manage_and_opens_it(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.press("ctrl-k", cx);
    for _ in 0..3 {
        palette.press("down", cx);
    }
    palette.press("enter", cx);
    assert_eq!(kind(&palette, cx), Some(Kind::History));
}

#[gpui_kit::test]
fn escape_returns_to_the_palette_with_the_same_search(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.type_text("a", cx);
    let before = palette.titles(cx);
    palette.press("f2", cx);
    palette.press("escape", cx);
    assert_eq!(kind(&palette, cx), None);
    assert_eq!(palette.query(cx), "a");
    assert_eq!(palette.titles(cx), before);
    assert!(palette.events.borrow().is_empty());
}

#[gpui_kit::test]
fn tab_walks_the_listed_kinds_only(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.press("f2", cx);
    let mut seen = Vec::new();
    for _ in 0..4 {
        palette.press("tab", cx);
        seen.push(kind(&palette, cx));
    }
    let listed = [Kind::Snippets, Kind::Paths, Kind::Secrets, Kind::History];
    assert_eq!(seen, listed.map(Some));
    palette.press("shift-tab", cx);
    assert_eq!(kind(&palette, cx), Some(Kind::Secrets));
}

#[gpui_kit::test]
fn delete_removes_the_selected_row_through_the_hooks(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx);
    palette.press("f2", cx);
    palette.press("down", cx);
    palette.press("delete", cx);
    assert_eq!(
        *fake.calls.lock().expect("calls"),
        [Call::Delete(ClipId(2))]
    );
    assert_eq!(titles(&palette, cx), ["alpha note"]);
    assert_eq!(
        palette.label("clip-palette-status", cx).as_deref(),
        Some("Deleted")
    );
}

#[gpui_kit::test]
fn delete_edits_the_search_while_one_is_typed(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx);
    palette.type_text("alpha", cx);
    palette.press("f2", cx);
    palette.press("delete", cx);
    assert!(fake.calls.lock().expect("calls").is_empty());
    assert_eq!(titles(&palette, cx), ["alpha note"]);
}

#[gpui_kit::test]
fn a_secret_lists_its_name_and_nothing_of_its_text(cx: &mut TestAppContext) {
    let (palette, _) = opened(cx);
    palette.press("f2", cx);
    for _ in 0..3 {
        palette.press("tab", cx);
    }
    assert_eq!(kind(&palette, cx), Some(Kind::Secrets));
    assert_eq!(titles(&palette, cx), ["epsilon login"]);
    let detail = palette.palette.read_with(cx, |palette, _| {
        let manage = palette.manage.as_ref().expect("open");
        manage.rows()[0].detail.clone()
    });
    assert!(detail.is_empty());
}

#[gpui_kit::test]
fn the_palette_keys_leave_the_hidden_palette_row_alone(cx: &mut TestAppContext) {
    let (palette, fake) = opened(cx);
    palette.press("f2", cx);
    palette.press("ctrl-k", cx);
    palette.press("enter", cx);
    palette.press("alt-r", cx);
    palette.press("ctrl-s", cx);
    assert!(fake.calls.lock().expect("calls").is_empty());
    assert!(palette.label("clip-palette-menu", cx).is_none());
}

#[gpui_kit::test]
fn hooks_that_offer_no_listed_mode_open_no_library(cx: &mut TestAppContext) {
    let fake = Fake::new(vec![ClipMode::All], library());
    let palette = open(cx, fake);
    palette.press("f2", cx);
    assert_eq!(kind(&palette, cx), None);
}

#[gpui_kit::test]
fn the_notes_kind_lists_note_rows_and_follows_the_search(cx: &mut TestAppContext) {
    let mut rows = library();
    rows.push(row(7, ClipKind::Note, "alpha", Origin::Saved));
    rows.push(row(8, ClipKind::Note, "Global note", Origin::Saved));
    let modes = [LIBRARY_MODES.as_slice(), &[ClipMode::Notes]].concat();
    let palette = open(cx, Fake::new(modes, rows));
    palette.press("f2", cx);
    assert_eq!(counts(&palette, cx)[5], Some(2));
    palette.press("shift-tab", cx);
    assert_eq!(kind(&palette, cx), Some(Kind::Notes));
    assert_eq!(titles(&palette, cx), ["alpha", "Global note"]);
    palette.type_text("Glob", cx);
    assert_eq!(titles(&palette, cx), ["Global note"]);
    assert_eq!(counts(&palette, cx)[5], Some(1));
}
