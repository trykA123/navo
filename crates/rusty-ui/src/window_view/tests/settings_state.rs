use std::time::Duration;

use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, ElementId, Hsla, TestAppContext};

use super::app_menu::{Rig, rig};
use crate::settings::{Change, FIELDS, Field, Values};
use crate::tokens::{self, Density, SyntaxTheme, ThemeChoice};
use crate::window_view::SettingsNews;

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

fn row_of(field: Field) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("every field has a row")
}

fn go_to(cx: &mut TestAppContext, rig: &Rig, field: Field) {
    let filter = crate::settings::Filter::default();
    let steps = FIELDS[..row_of(field)]
        .iter()
        .filter(|spec| filter.admits(spec))
        .count();
    for _ in 0..steps {
        rig.press(cx, "down");
    }
}

fn label(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> Option<String> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .filter(|found| found.visible())
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

fn state(cx: &mut TestAppContext, rig: &Rig, field: Field) -> Option<String> {
    label(cx, rig, ("settings-state", row_of(field)))
}

fn shows(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}

fn footer(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    label(cx, rig, "settings-footer")
}

fn footer_for(count: usize) -> Option<String> {
    Some(format!(
        "Applied live · Saved to ferrite.toml · {count} changed"
    ))
}

fn height_of_row(cx: &mut TestAppContext, rig: &Rig, row: usize) -> gpui_kit::Pixels {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find(("settings-row", row)).bounds().size.height
    })
    .expect("window")
}

fn send(cx: &mut TestAppContext, rig: &Rig, values: Values, generation: u64) {
    assert!(rig.news.send(SettingsNews::Applied { values, generation }));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn every_row_starts_at_default_with_a_live_chip(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert_eq!(state(cx, &rig, Field::Theme).as_deref(), Some("Default"));
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Default"));
    assert!(!shows(cx, &rig, ("settings-reset", row_of(Field::Density))));
    assert_eq!(label(cx, &rig, "settings-live").as_deref(), Some("Live"));
    assert_eq!(footer(cx, &rig), footer_for(0));
}

#[gpui_kit::test]
fn a_change_in_the_sheet_marks_its_row_changed_with_a_dot_and_reset(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    let row = row_of(Field::Density);
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Changed"));
    assert!(shows(cx, &rig, ("settings-dot", row)));
    assert!(shows(cx, &rig, ("settings-reset", row)));
    assert_eq!(state(cx, &rig, Field::Theme).as_deref(), Some("Default"));
    assert_eq!(footer(cx, &rig), footer_for(1));
}

#[gpui_kit::test]
fn a_hand_edit_marks_its_row_changed(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    let edited = Values::default().with(&Change::density(Density::Compact));
    send(cx, &rig, edited, 1);
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Changed"));
    assert_eq!(footer(cx, &rig), footer_for(1));
}

#[gpui_kit::test]
fn a_value_the_ledger_follows_stays_default_off_the_built_in_default(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    let following = Values::default()
        .with(&Change::density(Density::Compact))
        .marked(|_| false);
    send(cx, &rig, following, 1);
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Compact);
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Default"));
    assert_eq!(footer(cx, &rig), footer_for(0));
}

#[gpui_kit::test]
fn reset_returns_the_row_to_default_and_the_count_drops(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    rig.click(cx, ("settings-reset", row_of(Field::Density)));
    let last = rig.store.changes.lock().expect("changes").last().cloned();
    assert_eq!(last, Some(Change::reset(Field::Density)));
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Normal);
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Default"));
    assert!(!shows(cx, &rig, ("settings-reset", row_of(Field::Density))));
    assert_eq!(footer(cx, &rig), footer_for(0));
}

#[gpui_kit::test]
fn rows_keep_their_height_when_the_key_line_appears(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    let row = row_of(Field::Font);
    let away = height_of_row(cx, &rig, row);
    go_to(cx, &rig, Field::Font);
    assert!(shows(cx, &rig, "settings-key"));
    assert_eq!(height_of_row(cx, &rig, row), away);
}

fn last_change(rig: &Rig) -> Option<Change> {
    rig.store.changes.lock().expect("changes").last().cloned()
}

fn hint_labels(cx: &mut TestAppContext, rig: &Rig) -> Vec<String> {
    (0..8usize)
        .filter_map(|index| label(cx, rig, ("side-sheet-hint", index)))
        .collect()
}

#[gpui_kit::test]
fn ctrl_z_after_reset_brings_the_old_value_back_as_changed(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    rig.click(cx, ("settings-reset", row_of(Field::Density)));
    rig.press(cx, "ctrl-z");
    assert_eq!(last_change(&rig), Some(Change::density(Density::Compact)));
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Compact);
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Changed"));
}

#[gpui_kit::test]
fn ctrl_z_after_reset_of_a_key_without_a_default_writes_it_back(cx: &mut TestAppContext) {
    let values = Values::default().with(&Change::text(Field::EditorPath, "code"));
    let rig = super::app_menu::rig_on(cx, values, None);
    rig.store
        .present
        .lock()
        .expect("present")
        .push(Field::EditorPath);
    open(cx, &rig);
    go_to(cx, &rig, Field::EditorPath);
    rig.press(cx, "f8");
    assert_eq!(last_change(&rig), Some(Change::reset(Field::EditorPath)));
    assert_eq!(
        state(cx, &rig, Field::EditorPath).as_deref(),
        Some("Default")
    );
    rig.press(cx, "ctrl-z");
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::EditorPath, "code"))
    );
    assert_eq!(
        state(cx, &rig, Field::EditorPath).as_deref(),
        Some("Changed")
    );
}

#[gpui_kit::test]
fn f8_resets_a_changed_row_and_its_hint_shows_only_there(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    assert!(!hint_labels(cx, &rig).contains(&"Reset".to_owned()));
    let before = rig.store.changes.lock().expect("changes").len();
    rig.press(cx, "f8");
    assert_eq!(rig.store.changes.lock().expect("changes").len(), before);
    rig.press(cx, "left");
    assert!(hint_labels(cx, &rig).contains(&"Reset".to_owned()));
    rig.press(cx, "f8");
    assert_eq!(last_change(&rig), Some(Change::reset(Field::Density)));
    assert_eq!(state(cx, &rig, Field::Density).as_deref(), Some("Default"));
    assert!(!hint_labels(cx, &rig).contains(&"Reset".to_owned()));
}

fn keyword_colour(cx: &mut TestAppContext) -> Option<Hsla> {
    cx.read(|cx| {
        let theme = &KitTheme::global(cx).highlight_theme;
        theme.style("keyword").and_then(|style| style.color)
    })
}

#[gpui_kit::test]
fn a_syntax_theme_pick_in_the_sheet_recolours_the_highlight_theme_live(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::Light);
    open(cx, &rig);
    go_to(cx, &rig, Field::TextSyntaxTheme);
    let iron = keyword_colour(cx);
    assert!(iron.is_some());
    rig.press(cx, "right");
    let mocha = Change::syntax_theme(SyntaxTheme::Mocha);
    assert_eq!(last_change(&rig), Some(mocha));
    assert_eq!(cx.read(|cx| tokens::current(cx).syntax), SyntaxTheme::Mocha);
    assert_ne!(keyword_colour(cx), iron);
    rig.press(cx, "left");
    assert_eq!(
        cx.read(|cx| tokens::current(cx).syntax),
        SyntaxTheme::OxidisedIron
    );
    assert_eq!(keyword_colour(cx), iron);
}

#[gpui_kit::test]
fn a_hand_edited_syntax_theme_applies_live(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::Light);
    let iron = keyword_colour(cx);
    let edited = Values::default().with(&Change::syntax_theme(SyntaxTheme::Mocha));
    send(cx, &rig, edited, 1);
    assert_eq!(cx.read(|cx| tokens::current(cx).syntax), SyntaxTheme::Mocha);
    assert_ne!(keyword_colour(cx), iron);
}
