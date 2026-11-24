//! Settings v2 rows and inspector: one 36 px line per setting, and a column that shows Now,
//! Default, help, Reset and the TOML key of the focused row.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Bounds, ElementId, Pixels, TestAppContext, px};

use super::app_menu::{Rig, rig, rig_git_sized};
use super::git_menu::{Fake, menu_items, modified};
use crate::settings::{Change, FIELDS, Field, Section, Values, rows_in};
use crate::tokens::{self, Density, ThemeChoice};

const ROW: f32 = 36.;

fn open(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

fn row_of(field: Field) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("every field has a row")
}

/// Presses Down from the first row until `field` has focus; Down walks the rows in page order.
fn go_to(cx: &mut TestAppContext, rig: &Rig, field: Field) {
    let filter = crate::settings::Filter::default().hiding(Values::default().hidden_row());
    let steps = Section::ALL
        .into_iter()
        .flat_map(rows_in)
        .take_while(|(_, spec)| spec.field != field)
        .filter(|(_, spec)| filter.admits(spec))
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

fn bounds(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> Bounds<Pixels> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find(id).bounds()
    })
    .expect("window")
}

fn is_disabled(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> Option<bool> {
    let id = id.into();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|found| found.disabled())
    })
    .expect("window")
}

fn key_in_row(cx: &mut TestAppContext, rig: &Rig, row: usize) -> bool {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("settings-row", row))
            .try_find("settings-key")
            .is_some()
    })
    .expect("window")
}

fn last_change(rig: &Rig) -> Option<Change> {
    rig.store.changes.lock().expect("changes").last().cloned()
}

#[gpui_kit::test]
fn every_row_is_one_36_px_line_without_its_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    for field in [Field::Theme, Field::Density, Field::ColorTheme, Field::Font] {
        let row = row_of(field);
        let height = bounds(cx, &rig, ("settings-row", row)).size.height;
        assert_eq!(height, px(ROW), "{field:?}");
    }
    assert!(!key_in_row(cx, &rig, row_of(Field::Theme)));
}

#[gpui_kit::test]
fn the_inspector_shows_now_and_the_default_only_once_changed(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    let name = label(cx, &rig, "settings-inspector-name");
    assert_eq!(name.as_deref(), Some("Density"));
    assert_eq!(
        label(cx, &rig, "settings-inspector-now").as_deref(),
        Some("Normal")
    );
    assert_eq!(label(cx, &rig, "settings-inspector-default"), None);
    assert_eq!(
        label(cx, &rig, "settings-key").as_deref(),
        Some("appearance.density")
    );
    rig.press(cx, "left");
    assert_eq!(
        label(cx, &rig, "settings-inspector-now").as_deref(),
        Some("Compact")
    );
    assert_eq!(
        label(cx, &rig, "settings-inspector-default").as_deref(),
        Some("Normal")
    );
}

#[gpui_kit::test]
fn inspector_reset_is_disabled_at_default_and_resets_by_mouse(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    let reset = ("settings-reset", row_of(Field::Density));
    assert_eq!(is_disabled(cx, &rig, reset), Some(true));
    rig.press(cx, "left");
    assert_ne!(is_disabled(cx, &rig, reset), Some(true));
    rig.click(cx, reset);
    assert_eq!(last_change(&rig), Some(Change::reset(Field::Density)));
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Normal);
    assert_eq!(is_disabled(cx, &rig, reset), Some(true));
}

#[gpui_kit::test]
fn open_the_line_in_the_inspector_runs_the_f3_path(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    // The rig has no settings file, so F3 and the button both say so in the message line.
    assert_eq!(label(cx, &rig, "settings-error"), None);
    rig.click(cx, "settings-locate");
    assert_eq!(
        label(cx, &rig, "settings-error").as_deref(),
        Some("There is no settings file in this session.")
    );
}

fn open_sized(cx: &mut TestAppContext, width: f32) -> Rig {
    let git = Fake::new(menu_items(false), modified());
    let rig = rig_git_sized(cx, Values::default(), Arc::new(git), px(width));
    open(cx, &rig);
    rig
}

#[gpui_kit::test]
fn the_inspector_sits_beside_the_list_on_a_wide_dialog(cx: &mut TestAppContext) {
    let rig = open_sized(cx, 1400.);
    let rows = bounds(cx, &rig, "settings-rows");
    let inspector = bounds(cx, &rig, "settings-inspector");
    assert!(inspector.left() >= rows.right(), "{inspector:?} {rows:?}");
    assert_eq!(inspector.size.width, px(300.));
}

#[gpui_kit::test]
fn the_inspector_moves_under_the_list_on_a_small_dialog(cx: &mut TestAppContext) {
    let rig = open_sized(cx, 800.);
    let rows = bounds(cx, &rig, "settings-rows");
    let inspector = bounds(cx, &rig, "settings-inspector");
    assert!(inspector.top() >= rows.bottom(), "{inspector:?} {rows:?}");
    assert_eq!(inspector.left(), rows.left());
}

#[gpui_kit::test]
fn the_density_row_stays_one_line_on_the_narrowest_side_by_side_dialog(cx: &mut TestAppContext) {
    let rig = open_sized(cx, 960.);
    let rows = bounds(cx, &rig, "settings-rows");
    let inspector = bounds(cx, &rig, "settings-inspector");
    assert!(inspector.left() >= rows.right(), "{inspector:?} {rows:?}");
    let height = bounds(cx, &rig, ("settings-row", row_of(Field::Density)))
        .size
        .height;
    assert_eq!(height, px(ROW));
}
