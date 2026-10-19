use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use super::app_menu::{Rig, rig};
use crate::settings::{Change, FIELDS, Field, Values};
use crate::tokens::{self, Density, ThemeChoice};
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
    for _ in 0..row_of(field) {
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
