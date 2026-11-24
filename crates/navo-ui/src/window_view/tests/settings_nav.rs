//! Tests for the Settings nav: the greyed-out Modules page, the Ctrl+digit caps, and the
//! Keyboard page that holds both the modifier hints and the shortcut list.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, ElementId, Modifiers, TestAppContext};

use super::app_menu::{Rig, rig};
use super::fkeys::hold;
use crate::settings::{Field, Section};
use crate::tokens::ThemeChoice;

fn open(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    cx.executor().advance_clock(Duration::from_secs(1));
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

fn is_selected(cx: &mut TestAppContext, rig: &Rig, page: Section) -> bool {
    let index = Section::ALL.iter().position(|shown| *shown == page);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find(("settings-section", index.expect("a page")))
            .selected()
            == Some(true)
    })
    .expect("window")
}

fn row(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.view.read_with(cx, |view, _| view.settings_row())
}

#[gpui_kit::test]
fn modules_is_greyed_out_after_the_pages_and_shortcuts_has_no_tab(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert_eq!(
        label(cx, &rig, "settings-modules").as_deref(),
        Some("Modules")
    );
    assert_eq!(
        label(cx, &rig, ("settings-section", Section::ALL.len())),
        None
    );
    let before = row(cx, &rig);
    rig.click(cx, "settings-modules");
    assert_eq!(row(cx, &rig), before);
    assert!(is_selected(cx, &rig, Section::Appearance));
}

#[gpui_kit::test]
fn holding_ctrl_shows_each_page_digit(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    assert_eq!(label(cx, &rig, ("settings-digit", 0usize)), None);
    hold(cx, rig.window, Modifiers::control());
    assert_eq!(
        label(cx, &rig, ("settings-digit", 0usize)).as_deref(),
        Some("1")
    );
    assert_eq!(
        label(cx, &rig, ("settings-digit", 9usize)).as_deref(),
        Some("0")
    );
    hold(cx, rig.window, Modifiers::default());
    assert_eq!(label(cx, &rig, ("settings-digit", 0usize)), None);
}

#[gpui_kit::test]
fn down_from_the_last_keyboard_row_opens_the_shortcut_list_on_the_same_page(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    for _ in 0..8 {
        rig.press(cx, "tab");
    }
    assert_eq!(row(cx, &rig), Field::ModifierHints.index());
    rig.press(cx, "down");
    rig.press(cx, "down");
    assert!(
        rig.view
            .read_with(cx, |view, _| view.is_showing_shortcuts())
    );
    assert!(is_selected(cx, &rig, Section::Keyboard));
    assert!(label(cx, &rig, ("settings-row", Field::ModifierHints.index())).is_some());
    rig.press(cx, "tab");
    assert!(is_selected(cx, &rig, Section::About));
}

#[gpui_kit::test]
fn ctrl_digits_and_ctrl_tab_change_the_page(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    for (key, page) in [
        ("ctrl-3", Section::Files),
        ("ctrl-0", Section::About),
        ("ctrl-tab", Section::Appearance),
        ("ctrl-pagedown", Section::Window),
        ("ctrl-shift-tab", Section::Appearance),
        ("ctrl-pageup", Section::About),
        ("ctrl-9", Section::Keyboard),
    ] {
        rig.press(cx, key);
        assert!(is_selected(cx, &rig, page), "{key} shows {page:?}");
    }
    assert!(rig.view.read_with(cx, |view, _| view.settings.is_open()));
}
