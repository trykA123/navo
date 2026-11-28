//! Tests for the advanced folds in Settings: the arrow keys skip a closed fold, Right and a
//! click open it, Left closes it, and a Find pick inside a closed fold opens it.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, Target, rig};
use crate::settings::{Field, fold_row};
use crate::tokens::ThemeChoice;

fn settle(cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

/// Opens Settings on the Window page with the Date column row selected (the row above the
/// File list columns fold).
fn open_at_date_column(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    settle(cx);
    rig.press_in(cx, Target::Settings, "ctrl-2");
    let target = Field::WindowDateColumn.index();
    for _ in 0..30 {
        if row(cx, rig) == target {
            break;
        }
        rig.press_in(cx, Target::Settings, "down");
    }
    assert_eq!(row(cx, rig), target);
}

fn row(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.view.read_with(cx, |view, _| view.settings_row())
}

/// True when the list renders `id`, scrolled into view or not.
fn shows(cx: &mut TestAppContext, rig: &Rig, id: impl Into<gpui_kit::ElementId>) -> bool {
    let Some(window) = rig.settings_window(cx) else {
        return false;
    };
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .expect("settings window")
}

fn shows_row(cx: &mut TestAppContext, rig: &Rig, field: Field) -> bool {
    shows(cx, rig, ("settings-row", field.index()))
}

fn columns_fold() -> usize {
    fold_row(Field::WindowExtWidth).expect("the column widths are advanced")
}

#[gpui_kit::test]
fn down_stops_on_a_closed_fold_and_then_skips_its_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    assert!(shows(cx, &rig, ("settings-fold", columns_fold())));
    assert!(!shows_row(cx, &rig, Field::WindowExtWidth));
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), columns_fold());
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), Field::WindowFreeSpace.index());
    rig.press_in(cx, Target::Settings, "up");
    assert_eq!(row(cx, &rig), columns_fold());
}

#[gpui_kit::test]
fn right_opens_the_fold_and_down_walks_into_it(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "down");
    rig.press_in(cx, Target::Settings, "right");
    assert_eq!(row(cx, &rig), columns_fold());
    assert!(shows_row(cx, &rig, Field::WindowExtWidth));
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), Field::WindowExtWidth.index());
}

#[gpui_kit::test]
fn left_inside_an_open_fold_closes_it_and_selects_the_fold_row(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "down");
    rig.press_in(cx, Target::Settings, "right");
    rig.press_in(cx, Target::Settings, "down");
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), Field::WindowSizeWidth.index());
    let width = rig.view.read_with(cx, |view, _| view.values.clone());
    rig.press_in(cx, Target::Settings, "left");
    assert_eq!(row(cx, &rig), columns_fold());
    assert!(!shows_row(cx, &rig, Field::WindowSizeWidth));
    rig.view
        .read_with(cx, |view, _| assert_eq!(view.values, width));
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), Field::WindowFreeSpace.index());
}

#[gpui_kit::test]
fn a_find_pick_inside_a_closed_fold_opens_it(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "ctrl-f");
    rig.input_in(cx, Target::Settings, "window.view.date-width");
    settle(cx);
    rig.click_in(
        cx,
        Target::Settings,
        ("settings-row", Field::WindowDateWidth.index()),
    );
    rig.press_in(cx, Target::Settings, "escape");
    settle(cx);
    assert_eq!(row(cx, &rig), Field::WindowDateWidth.index());
    assert!(shows_row(cx, &rig, Field::WindowDateWidth));
    rig.press_in(cx, Target::Settings, "left");
    assert_eq!(row(cx, &rig), columns_fold());
}

#[gpui_kit::test]
fn a_click_toggles_the_fold_and_closing_settings_closes_it(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "down");
    rig.click_in(cx, Target::Settings, ("settings-fold", columns_fold()));
    assert_eq!(row(cx, &rig), columns_fold());
    assert!(shows_row(cx, &rig, Field::WindowExtWidth));
    rig.click_in(cx, Target::Settings, ("settings-fold", columns_fold()));
    assert!(!shows_row(cx, &rig, Field::WindowExtWidth));
    rig.click_in(cx, Target::Settings, ("settings-fold", columns_fold()));
    rig.press_in(cx, Target::Settings, "escape");
    settle(cx);
    rig.press(cx, "ctrl-,");
    settle(cx);
    assert!(!shows_row(cx, &rig, Field::WindowExtWidth));
}

#[gpui_kit::test]
fn escape_from_a_find_with_no_match_keeps_the_fold_row(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), columns_fold());
    rig.press_in(cx, Target::Settings, "ctrl-f");
    rig.input_in(cx, Target::Settings, "zzqxnomatch");
    settle(cx);
    assert_eq!(row(cx, &rig), columns_fold());
    rig.press_in(cx, Target::Settings, "escape");
    settle(cx);
    assert_eq!(row(cx, &rig), columns_fold());
}

#[gpui_kit::test]
fn the_inspector_on_a_fold_row_lists_its_settings_and_a_click_opens_one(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "down");
    assert_eq!(row(cx, &rig), columns_fold());
    assert!(shows(cx, &rig, "settings-inspector-fold"));
    let size = (
        "settings-inspector-advanced",
        Field::WindowSizeWidth.index(),
    );
    assert!(shows(cx, &rig, size));
    assert!(!shows(
        cx,
        &rig,
        (
            "settings-inspector-advanced",
            Field::WindowFreeSpace.index()
        )
    ));
    rig.click_in(cx, Target::Settings, size);
    assert_eq!(row(cx, &rig), Field::WindowSizeWidth.index());
    assert!(shows_row(cx, &rig, Field::WindowSizeWidth));
}

#[gpui_kit::test]
fn find_tags_advanced_results_only(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "ctrl-f");
    rig.input_in(cx, Target::Settings, "window.view.date");
    settle(cx);
    assert!(shows_row(cx, &rig, Field::WindowDateColumn));
    let tag = |field: Field| ("settings-advanced-tag", field.index());
    assert!(shows(cx, &rig, tag(Field::WindowDateWidth)));
    assert!(!shows(cx, &rig, tag(Field::WindowDateColumn)));
    rig.click_in(
        cx,
        Target::Settings,
        ("settings-row", Field::WindowDateWidth.index()),
    );
    rig.press_in(cx, Target::Settings, "escape");
    settle(cx);
    assert!(shows_row(cx, &rig, Field::WindowDateWidth));
    assert!(!shows(cx, &rig, tag(Field::WindowDateWidth)));
}

#[gpui_kit::test]
fn undo_restores_a_value_and_reopens_its_closed_fold(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at_date_column(cx, &rig);
    rig.press_in(cx, Target::Settings, "down");
    rig.click_in(
        cx,
        Target::Settings,
        (
            "settings-inspector-advanced",
            Field::WindowSizeWidth.index(),
        ),
    );
    let before = rig.view.read_with(cx, |view, _| view.values.clone());
    rig.press_in(cx, Target::Settings, "right");
    settle(cx);
    assert_ne!(
        rig.view.read_with(cx, |view, _| view.values.clone()),
        before
    );
    rig.press_in(cx, Target::Settings, "left");
    assert_eq!(row(cx, &rig), columns_fold());
    assert!(!shows_row(cx, &rig, Field::WindowSizeWidth));
    rig.press_in(cx, Target::Settings, "ctrl-z");
    settle(cx);
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.values.clone()),
        before
    );
    assert_eq!(row(cx, &rig), Field::WindowSizeWidth.index());
    assert!(shows_row(cx, &rig, Field::WindowSizeWidth));
}
