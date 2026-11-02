use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, rig};
use crate::app_menu::{Item, MenuCommand, index_of};
use crate::settings::{FIELDS, Section};
use crate::tokens::ThemeChoice;
use crate::window_view::tests::{fake, open_options, options_for, two_panes, viewer_open};

fn open_help(cx: &mut TestAppContext, rig: &Rig) {
    rig.press(cx, "ctrl-,");
    let last = Section::ALL.len() - 1;
    rig.click(cx, ("settings-section", last));
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view.read_with(cx, |view, _| view.settings.is_open())
}

fn has_shortcuts(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view
        .read_with(cx, |view, _| view.is_showing_shortcuts())
}

fn label_of(cx: &mut TestAppContext, rig: &Rig, id: &'static str) -> Option<String> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

#[gpui_kit::test]
fn help_is_the_last_section_with_both_buttons_and_the_version(cx: &mut TestAppContext) {
    assert_eq!(Section::ALL.last(), Some(&Section::Help));
    let rig = rig(cx, ThemeChoice::System);
    open_help(cx, &rig);
    let row = rig.view.read_with(cx, |view, _| view.settings_row());
    assert_eq!(row, FIELDS.len() + 1);
    assert_eq!(
        label_of(cx, &rig, "settings-help-guide").as_deref(),
        Some("User guide")
    );
    assert_eq!(
        label_of(cx, &rig, "settings-help-keys").as_deref(),
        Some("Keyboard shortcuts")
    );
    assert_eq!(
        label_of(cx, &rig, "settings-help-keys-hint").as_deref(),
        Some("F1")
    );
    assert_eq!(
        label_of(cx, &rig, "settings-help-version").as_deref(),
        Some("Navo 0.0.1")
    );
}

#[gpui_kit::test]
fn the_user_guide_button_closes_settings_and_opens_the_guide(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_help(cx, &rig);
    rig.click(cx, "settings-help-guide");
    assert!(!is_open(cx, &rig));
    assert!(viewer_open(cx, &rig.view));
    rig.press(cx, "escape");
    assert!(!viewer_open(cx, &rig.view));
}

#[gpui_kit::test]
fn the_shortcuts_button_shows_the_key_map_page(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_help(cx, &rig);
    assert!(!has_shortcuts(cx, &rig));
    rig.click(cx, "settings-help-keys");
    assert!(is_open(cx, &rig));
    assert!(has_shortcuts(cx, &rig));
}

#[gpui_kit::test]
fn f1_opens_settings_on_the_key_map_page(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f1");
    assert!(is_open(cx, &rig));
    assert!(has_shortcuts(cx, &rig));
}

#[gpui_kit::test]
fn the_help_row_of_the_app_menu_opens_the_key_map_page(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    let row = index_of(Item::Row(MenuCommand::Help)).expect("help row");
    rig.click(cx, ("app-menu-row", row));
    assert!(is_open(cx, &rig));
    assert!(has_shortcuts(cx, &rig));
}

#[gpui_kit::test]
fn the_first_start_notice_shows_once_per_profile(cx: &mut TestAppContext) {
    let shown = Arc::new(AtomicUsize::new(0));
    for expected in [true, false] {
        let fs = fake();
        let mut options = options_for(fs.clone(), two_panes(&fs));
        let first = shown.clone();
        options.settings.take_guide_hint =
            Arc::new(move || first.fetch_add(1, Ordering::SeqCst) == 0);
        let (_, view) = open_options(cx, options);
        cx.run_until_parked();
        let notice = view.read_with(cx, |view, _| view.status_notice().map(|n| n.text));
        let expected_text = expected.then_some("New to Navo? Shift+F1 opens the user guide.");
        assert_eq!(notice.as_deref(), expected_text);
    }
}
