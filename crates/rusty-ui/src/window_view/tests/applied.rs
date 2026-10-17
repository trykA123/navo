use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, rig, rig_on};
use crate::settings::{Change, Field, SORTS, Value, Values};
use crate::tokens::ThemeChoice;

fn names(cx: &mut TestAppContext, rig: &Rig) -> Vec<String> {
    rig.pane_names(cx).into_iter().next().unwrap_or_default()
}

fn open_at(cx: &mut TestAppContext, rig: &Rig, field: Field) {
    rig.press(cx, "ctrl-,");
    let row = crate::settings::FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("a row");
    for _ in 0..row {
        rig.press(cx, "down");
    }
}

fn sort_by(name: &str) -> Change {
    let at = SORTS
        .iter()
        .position(|choice| choice.label == name)
        .expect("a sort");
    Change {
        field: Field::Sort,
        value: Value::Pick(at),
    }
}

#[gpui_kit::test]
fn sort_by_size_reorders_the_live_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert_eq!(names(cx, &rig), ["docs", "a.txt", "b-secret.txt"]);
    open_at(cx, &rig, Field::Sort);
    rig.press(cx, "right");
    assert_eq!(names(cx, &rig), ["docs", "b-secret.txt", "a.txt"]);
}

#[gpui_kit::test]
fn the_saved_sort_orders_the_panes_at_start(cx: &mut TestAppContext) {
    let values = Values::default().with(&sort_by("Size"));
    let rig = rig_on(cx, values, None);
    assert_eq!(names(cx, &rig), ["docs", "b-secret.txt", "a.txt"]);
}

#[gpui_kit::test]
fn descending_reverses_the_live_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at(cx, &rig, Field::SortDescending);
    rig.press(cx, "space");
    assert_eq!(names(cx, &rig), ["docs", "b-secret.txt", "a.txt"]);
}

#[gpui_kit::test]
fn folders_first_off_mixes_folders_into_the_live_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at(cx, &rig, Field::FoldersFirst);
    rig.press(cx, "space");
    assert_eq!(names(cx, &rig), ["a.txt", "b-secret.txt", "docs"]);
}

#[gpui_kit::test]
fn show_system_files_shows_them_in_the_live_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert!(!names(cx, &rig).contains(&"c-boot.ini".to_owned()));
    open_at(cx, &rig, Field::ShowSystem);
    rig.press(cx, "space");
    assert!(names(cx, &rig).contains(&"c-boot.ini".to_owned()));
}

#[gpui_kit::test]
fn space_stops_measuring_folders_when_that_row_is_off(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at(cx, &rig, Field::SpaceMeasuresFolders);
    rig.press(cx, "left");
    rig.press(cx, "escape");
    rig.press(cx, "space");
    let size = rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.details(tab).first().map(|entry| entry.size)
    });
    assert_ne!(size, Some(7));
}

#[gpui_kit::test]
fn patterns_select_folders_when_that_row_is_on(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_at(cx, &rig, Field::PatternsSelectFolders);
    rig.press(cx, "right");
    rig.press(cx, "escape");
    rig.press(cx, "+");
    cx.update_window(rig.window, |_, window, cx| window.input("d*", cx))
        .expect("window");
    rig.press(cx, "enter");
    let marked = rig.view.read_with(cx, |view, _| {
        view.browser.state().active_tab().selection().is_marked(0)
    });
    assert!(marked, "docs is marked");
}

#[gpui_kit::test]
fn rows_without_a_consumer_say_they_are_not_applied_yet(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "ctrl-,");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let mut note = |row: usize| {
            window
                .within(("settings-row", row))
                .try_find("settings-not-applied")
                .and_then(|found| found.label().map(str::to_owned))
        };
        assert_eq!(note(4).as_deref(), Some("Not applied yet"));
        assert_eq!(note(1), None);
    })
    .expect("window");
}
