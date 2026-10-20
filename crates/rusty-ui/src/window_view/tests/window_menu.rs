use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::shell_menu::MenuItem;

use super::app_menu::{Rig, rig_on};
use super::window_parts::{every_part, off};
use crate::settings::{Change, Field, Value, Values, WindowParts};
use crate::tokens::Density;

fn items(cx: &mut TestAppContext, rig: &Rig) -> Option<Vec<MenuItem>> {
    rig.view.read_with(cx, |view, cx| {
        let menu = view.window_menu_view()?;
        Some(menu.read(cx).menu().items.clone())
    })
}

fn checked(cx: &mut TestAppContext, rig: &Rig, text: &str) -> Option<bool> {
    let all = items(cx, rig)?;
    let nested = all.iter().filter_map(|item| item.children()).flatten();
    all.iter()
        .chain(nested)
        .find(|item| item.text == text)
        .map(|item| item.checked)
}

fn pick(cx: &mut TestAppContext, rig: &Rig, text: &str) {
    cx.update_window(rig.window, |_, window, cx| window.input(text, cx))
        .expect("window");
    cx.run_until_parked();
    rig.press(cx, "enter");
    cx.run_until_parked();
}

fn is_drawn(cx: &mut TestAppContext, rig: &Rig, id: &'static str) -> bool {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .expect("window")
}

fn stored_parts(rig: &Rig) -> WindowParts {
    WindowParts::of(&rig.store.stored.lock().expect("stored"))
}

#[gpui_kit::test]
fn alt_v_in_the_file_list_opens_the_window_menu(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    assert!(items(cx, &rig).is_none());
    rig.press(cx, "alt-v");
    let shown = items(cx, &rig).expect("the menu opens");
    let texts: Vec<&str> = shown.iter().map(|item| item.text.as_str()).collect();
    for text in [
        "Window",
        "Path bar",
        "Columns",
        "Extension column",
        "Pane footer",
        "Free space",
        "Files",
        "Density",
        "Key hints",
        "Reset to defaults",
    ] {
        assert!(texts.contains(&text), "{text} in {texts:?}");
    }
    assert!(!texts.contains(&"Name column"));
    assert_eq!(checked(cx, &rig, "Path bar"), Some(true));
    assert_eq!(checked(cx, &rig, "Hidden files"), Some(true));
    assert_eq!(checked(cx, &rig, "Folders first"), Some(true));
    assert_eq!(checked(cx, &rig, "Show system files"), Some(false));
    rig.press(cx, "escape");
    assert!(items(cx, &rig).is_none());
}

#[gpui_kit::test]
fn the_window_groups_come_before_the_other_settings(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    rig.press(cx, "alt-v");
    let shown = items(cx, &rig).expect("the menu opens");
    let at = |text: &str| shown.iter().position(|item| item.text == text).expect(text);
    assert!(at("Window") < at("Columns") && at("Columns") < at("Pane footer"));
    assert!(at("Pane footer") < at("Files") && at("Files") < at("Reset to defaults"));
    assert!(shown.iter().filter(|item| !item.enabled).count() >= 3);
}

#[gpui_kit::test]
fn the_view_button_in_the_title_bar_opens_the_same_menu(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    rig.click(cx, "window-view");
    assert!(items(cx, &rig).is_some());
}

#[gpui_kit::test]
fn a_toggle_saves_at_once_and_the_menu_stays_open(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    rig.press(cx, "alt-v");
    pick(cx, &rig, "path bar");
    let expected = Change {
        field: Field::WindowPathBar,
        value: Value::Flag(false),
    };
    assert_eq!(
        rig.store.changes.lock().expect("changes").clone(),
        [expected]
    );
    assert!(!stored_parts(&rig).path_bar);
    assert_eq!(checked(cx, &rig, "Path bar"), Some(false));
    assert!(!is_drawn(cx, &rig, "path"));
}

#[gpui_kit::test]
fn a_toggle_of_an_existing_setting_saves_through_the_same_path(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    rig.press(cx, "alt-v");
    pick(cx, &rig, "show system files");
    let changes = rig.store.changes.lock().expect("changes").clone();
    assert_eq!(
        changes,
        [Change {
            field: Field::ShowSystem,
            value: Value::Flag(true)
        }]
    );
}

#[gpui_kit::test]
fn a_density_choice_saves_and_closes_the_menu(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    rig.press(cx, "alt-v");
    pick(cx, &rig, "compact");
    let changes = rig.store.changes.lock().expect("changes").clone();
    assert_eq!(changes, [Change::density(Density::Compact)]);
    assert!(items(cx, &rig).is_none());
}

#[gpui_kit::test]
fn reset_puts_every_window_part_back_and_leaves_other_settings(cx: &mut TestAppContext) {
    let hidden = off(&every_part()).with(&Change::density(Density::Compact));
    let rig = rig_on(cx, hidden, None);
    assert_eq!(stored_parts(&rig), WindowParts::all(false));
    rig.press(cx, "alt-v");
    pick(cx, &rig, "reset to defaults");
    assert_eq!(stored_parts(&rig), WindowParts::default());
    let changes = rig.store.changes.lock().expect("changes").clone();
    assert_eq!(changes.len(), 11);
    assert!(changes.iter().all(|change| change.value == Value::Reset));
    let stored = rig.store.stored.lock().expect("stored").clone();
    assert_eq!(stored.density(), Density::Compact);
    assert!(items(cx, &rig).is_none());
    assert!(!is_drawn(cx, &rig, "fkeys") && is_drawn(cx, &rig, "title-bar-search"));
}

#[gpui_kit::test]
fn reset_with_nothing_changed_saves_nothing(cx: &mut TestAppContext) {
    let rig = rig_on(cx, Values::default(), None);
    rig.press(cx, "alt-v");
    pick(cx, &rig, "reset to defaults");
    assert!(rig.store.changes.lock().expect("changes").is_empty());
}
