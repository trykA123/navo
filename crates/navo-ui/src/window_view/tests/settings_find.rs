use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, rig};
use super::type_in;
use crate::settings::{FIELDS, Field};
use crate::tokens::ThemeChoice;

fn settle_motion(cx: &mut TestAppContext, rig: &Rig) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

fn open(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    settle_motion(cx, rig);
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view.read_with(cx, |view, _| view.settings.is_open())
}

fn row_of(field: Field) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("every field has a row")
}

fn shows(cx: &mut TestAppContext, rig: &Rig, id: impl Into<gpui_kit::ElementId>) -> bool {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

fn shows_row(cx: &mut TestAppContext, rig: &Rig, field: Field) -> bool {
    shows(cx, rig, ("settings-row", row_of(field)))
}

fn current_row(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.view.read_with(cx, |view, _| view.settings_row())
}

fn find_text(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find("settings-find").value().map(str::to_owned)
    })
    .expect("window")
}

fn find(cx: &mut TestAppContext, rig: &Rig, text: &str) {
    rig.press(cx, "ctrl-f");
    type_in(cx, rig.window, &rig.view, text);
    settle_motion(cx, rig);
}

#[gpui_kit::test]
fn find_filters_rows_by_label(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert!(shows_row(cx, &rig, Field::Theme));
    find(cx, &rig, "FOLDERS first");
    assert!(shows_row(cx, &rig, Field::FoldersFirst));
    assert!(!shows_row(cx, &rig, Field::Theme));
    assert!(!shows_row(cx, &rig, Field::ShowSystem));
}

#[gpui_kit::test]
fn find_filters_rows_by_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "appearance.density");
    assert!(shows_row(cx, &rig, Field::Density));
    assert!(!shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn find_filters_rows_by_help_text(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "row height");
    assert!(shows_row(cx, &rig, Field::Density));
    assert!(!shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn find_lists_matches_from_every_section_and_hides_empty_headings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "line numbers");
    assert!(shows_row(cx, &rig, Field::TextLineNumbers));
    assert!(shows(cx, &rig, "settings-group-text"));
    assert!(!shows(cx, &rig, "settings-group-window"));
    assert!(!shows(cx, &rig, "settings-group-image"));
    assert!(!shows(cx, &rig, "settings-no-match"));
}

#[gpui_kit::test]
fn find_moves_the_row_to_the_first_match_and_arrows_stay_on_matches(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "files.sort");
    assert_eq!(current_row(cx, &rig), row_of(Field::Sort));
    rig.press(cx, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::Sort));
    rig.press(cx, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::SortDescending));
    rig.press(cx, "down");
    assert_eq!(current_row(cx, &rig), row_of(Field::SortDescending));
    rig.press(cx, "up");
    assert_eq!(current_row(cx, &rig), row_of(Field::Sort));
}

#[gpui_kit::test]
fn no_match_shows_a_short_message(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "qqqq");
    assert!(shows(cx, &rig, "settings-no-match"));
    assert!(!shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn escape_clears_the_find_and_a_second_escape_closes(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "density");
    assert!(!shows_row(cx, &rig, Field::Theme));
    rig.press(cx, "escape");
    settle_motion(cx, &rig);
    assert!(is_open(cx, &rig));
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::Theme));
    rig.press(cx, "escape");
    assert!(!is_open(cx, &rig));
}

#[gpui_kit::test]
fn typing_in_the_list_does_not_fill_the_find_or_change_rows(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    type_in(cx, rig.window, &rig.view, "sort");
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::Theme));
    assert_eq!(current_row(cx, &rig), 0);
}

#[gpui_kit::test]
fn closing_the_sheet_clears_the_find(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    find(cx, &rig, "density");
    rig.press(cx, "escape");
    rig.press(cx, "escape");
    open(cx, &rig);
    assert_eq!(find_text(cx, &rig).as_deref(), Some(""));
    assert!(shows_row(cx, &rig, Field::Theme));
}

#[gpui_kit::test]
fn keys_change_nothing_when_no_row_matches_the_find(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::Dark);
    open(cx, &rig);
    find(cx, &rig, "zzz");
    let before = rig.view.read_with(cx, |view, _| view.values.clone());
    rig.press(cx, "down");
    for key in ["space", "left", "right", "f8"] {
        rig.press(cx, key);
    }
    settle_motion(cx, &rig);
    assert!(rig.store.changes.lock().expect("changes").is_empty());
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.values.clone()),
        before
    );
}
