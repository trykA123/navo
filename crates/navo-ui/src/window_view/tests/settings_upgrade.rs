use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use super::app_menu::{Rig, rig};
use crate::settings::Values;
use crate::settings::{Change, DefaultShift, Field, Value};
use crate::tokens::ThemeChoice;
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

fn shift(key: &str, old: &str, new: &str) -> DefaultShift {
    DefaultShift {
        key: key.to_owned(),
        old: old.to_owned(),
        new: new.to_owned(),
    }
}

fn sort_shift() -> DefaultShift {
    shift("files.sort", "\"date\"", "\"name\"")
}

fn send(cx: &mut TestAppContext, rig: &Rig, shifts: Vec<DefaultShift>) {
    assert!(rig.news.send(SettingsNews::Defaults(shifts)));
    cx.run_until_parked();
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

fn shows(cx: &mut TestAppContext, rig: &Rig, id: impl Into<ElementId>) -> bool {
    label(cx, rig, id).is_some()
}

fn status(cx: &mut TestAppContext, rig: &Rig) -> String {
    let pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    label(cx, rig, ("status", pane.get())).unwrap_or_default()
}

#[gpui_kit::test]
fn the_notice_lists_each_change_with_its_label_and_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(
        cx,
        &rig,
        vec![sort_shift(), shift("files.folders-first", "false", "true")],
    );
    open(cx, &rig);
    assert_eq!(
        label(cx, &rig, "settings-upgrade").as_deref(),
        Some("Defaults changed")
    );
    assert_eq!(
        label(cx, &rig, ("settings-upgrade-item", 0usize)).as_deref(),
        Some("Sort by, files.sort: date → name")
    );
    assert_eq!(
        label(cx, &rig, ("settings-upgrade-item", 1usize)).as_deref(),
        Some("Folders first, files.folders-first: false → true")
    );
}

#[gpui_kit::test]
fn there_is_no_notice_without_changes(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
}

#[gpui_kit::test]
fn keep_old_value_saves_the_old_value_and_drops_that_item(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(
        cx,
        &rig,
        vec![sort_shift(), shift("files.folders-first", "false", "true")],
    );
    open(cx, &rig);
    rig.click(cx, ("settings-keep", 0usize));
    settle_motion(cx, &rig);
    let last = rig.store.changes.lock().expect("changes").last().cloned();
    assert_eq!(
        last,
        Some(Change {
            field: Field::Sort,
            value: Value::Pick(2)
        })
    );
    assert_eq!(
        label(cx, &rig, ("settings-upgrade-item", 0usize)).as_deref(),
        Some("Folders first, files.folders-first: false → true")
    );
    assert!(!shows(cx, &rig, ("settings-upgrade-item", 1usize)));
}

#[gpui_kit::test]
fn dismiss_hides_the_notice_and_the_same_changes_do_not_return(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    open(cx, &rig);
    rig.click(cx, "settings-dismiss");
    settle_motion(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
    send(cx, &rig, vec![sort_shift()]);
    settle_motion(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
    assert!(rig.store.changes.lock().expect("changes").is_empty());
}

#[gpui_kit::test]
fn a_different_change_after_a_dismiss_shows_again(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    open(cx, &rig);
    rig.click(cx, "settings-dismiss");
    send(
        cx,
        &rig,
        vec![shift("files.folders-first", "false", "true")],
    );
    settle_motion(cx, &rig);
    assert!(shows(cx, &rig, "settings-upgrade"));
    assert!(!shows(cx, &rig, ("settings-upgrade-item", 1usize)));
}

#[gpui_kit::test]
fn a_change_for_a_key_without_a_row_is_ignored(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![shift("nothing.here", "1", "2")]);
    open(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
}

#[gpui_kit::test]
fn the_status_line_points_to_a_pending_notice_until_it_is_dismissed(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    assert!(status(cx, &rig).contains("Defaults changed for 1 setting"));
    open(cx, &rig);
    rig.click(cx, "settings-dismiss");
    rig.press(cx, "escape");
    settle_motion(cx, &rig);
    assert!(!status(cx, &rig).contains("Defaults changed"));
}

fn last_change(rig: &Rig) -> Option<Change> {
    rig.store.changes.lock().expect("changes").last().cloned()
}

fn sort_row(cx: &mut TestAppContext, rig: &Rig) {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_at(Field::Sort, window, cx)
        });
    })
    .expect("window");
    settle_motion(cx, rig);
}

#[gpui_kit::test]
fn picking_another_value_in_that_row_drops_the_item(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    open(cx, &rig);
    sort_row(cx, &rig);
    rig.press(cx, "right");
    settle_motion(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
}

#[gpui_kit::test]
fn a_file_edit_to_that_key_drops_the_item(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    open(cx, &rig);
    assert!(shows(cx, &rig, "settings-upgrade"));
    let edited = Values::default()
        .with(&Change {
            field: Field::Sort,
            value: Value::Pick(1),
        })
        .with(&Change::show_hidden(true));
    assert!(rig.news.send(SettingsNews::Applied {
        values: edited,
        generation: 1
    }));
    settle_motion(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
}

#[gpui_kit::test]
fn a_default_already_changed_in_the_sheet_shows_no_item(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    sort_row(cx, &rig);
    rig.press(cx, "right");
    settle_motion(cx, &rig);
    send(cx, &rig, vec![sort_shift()]);
    settle_motion(cx, &rig);
    assert!(!shows(cx, &rig, "settings-upgrade"));
}

#[gpui_kit::test]
fn ctrl_z_undoes_keep_old_value(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    open(cx, &rig);
    rig.click(cx, ("settings-keep", 0usize));
    settle_motion(cx, &rig);
    assert_eq!(last_change(&rig).map(|c| c.value), Some(Value::Pick(2)));
    rig.press(cx, "ctrl-z");
    settle_motion(cx, &rig);
    assert_eq!(
        last_change(&rig),
        Some(Change {
            field: Field::Sort,
            value: Value::Unset
        })
    );
    let sort = rig
        .view
        .read_with(cx, |view, _| view.values.get(Field::Sort).clone());
    assert_eq!(sort, Value::Pick(0));
}

#[gpui_kit::test]
fn a_failed_save_keeps_the_item_and_shows_the_problem(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    send(cx, &rig, vec![sort_shift()]);
    open(cx, &rig);
    *rig.store.failure.lock().expect("failure") = Some("disk full".to_owned());
    rig.click(cx, ("settings-keep", 0usize));
    settle_motion(cx, &rig);
    assert!(shows(cx, &rig, ("settings-upgrade-item", 0usize)));
    assert!(rig.view.read_with(cx, |view, _| view.saver.error.is_some()));
}
