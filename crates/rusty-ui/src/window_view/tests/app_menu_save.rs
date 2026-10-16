use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{rig, rig_full};
use crate::app_menu::QuickSettings;
use crate::settings::Change;
use crate::tokens::{self, Density, ThemeChoice};

fn quick(cx: &mut TestAppContext, rig: &super::app_menu::Rig) -> QuickSettings {
    rig.view.read_with(cx, |view, _| view.values.quick())
}

#[gpui_kit::test]
fn a_tile_saves_only_its_own_setting(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    rig.click(cx, ("app-menu-tile", 1usize));
    let changes = rig.store.changes.lock().expect("changes").clone();
    assert_eq!(changes, [Change::density(Density::Comfortable)]);
}

#[gpui_kit::test]
fn a_click_never_saves_on_the_ui_thread(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    cx.update_window(rig.window, |_, window, cx| {
        window.click(("app-menu-tile", 0usize), cx);
    })
    .expect("window");
    assert!(rig.store.changes.lock().expect("changes").is_empty());
    assert_eq!(quick(cx, &rig).theme, ThemeChoice::Light);
    cx.run_until_parked();
    assert_eq!(rig.store.changes.lock().expect("changes").len(), 1);
}

#[gpui_kit::test]
fn rapid_clicks_coalesce_and_the_latest_value_wins(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "f10");
    cx.update_window(rig.window, |_, window, cx| {
        for _ in 0..4 {
            window.click(("app-menu-tile", 0usize), cx);
        }
    })
    .expect("window");
    cx.run_until_parked();
    let writes = rig.store.changes.lock().expect("changes").len();
    assert!((1..=2).contains(&writes), "{writes} writes");
    let stored = rig.store.stored.lock().expect("stored").clone();
    assert_eq!(stored.theme(), ThemeChoice::Light);
    assert_eq!(quick(cx, &rig).theme, ThemeChoice::Light);
}

#[gpui_kit::test]
fn values_reloaded_from_the_store_replace_the_quick_values(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    {
        let mut stored = rig.store.stored.lock().expect("stored");
        *stored = stored.with(&Change::density(Density::Compact));
    }
    rig.press(cx, "f10");
    rig.click(cx, ("app-menu-tile", 0usize));
    let now = quick(cx, &rig);
    assert_eq!(now.theme, ThemeChoice::Light);
    assert_eq!(now.density, Density::Compact);
    assert_eq!(cx.read(|cx| tokens::current(cx).density), Density::Compact);
}

fn error_lines(
    cx: &mut TestAppContext,
    rig: &super::app_menu::Rig,
) -> (Option<String>, Option<String>) {
    let pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let menu = window
            .try_find("app-menu-error")
            .and_then(|e| e.label().map(str::to_owned));
        let line = window
            .try_find(("pane-error", pane.get()))
            .and_then(|e| e.label().map(str::to_owned));
        (menu, line)
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_failed_save_is_shown_and_the_choice_stays_for_the_session(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    *rig.store.failure.lock().expect("failure") = Some("ferrite.toml: disk full".to_owned());
    rig.press(cx, "f10");
    rig.click(cx, ("app-menu-tile", 1usize));
    assert_eq!(quick(cx, &rig).density, Density::Comfortable);
    let (menu, line) = error_lines(cx, &rig);
    assert_eq!(menu.as_deref(), Some("ferrite.toml: disk full"));
    assert_eq!(line.as_deref(), Some("ferrite.toml: disk full"));
}

#[gpui_kit::test]
fn an_invalid_settings_file_is_reported_at_start(cx: &mut TestAppContext) {
    let settings = QuickSettings {
        show_hidden: true,
        ..QuickSettings::default()
    };
    let rig = rig_full(cx, settings, Some("ferrite.toml is invalid at line 2"));
    let (_, line) = error_lines(cx, &rig);
    assert_eq!(line.as_deref(), Some("ferrite.toml is invalid at line 2"));
    rig.press(cx, "f10");
    let (menu, _) = error_lines(cx, &rig);
    assert_eq!(menu.as_deref(), Some("ferrite.toml is invalid at line 2"));
}

#[gpui_kit::test]
fn a_good_save_clears_the_error(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    *rig.store.failure.lock().expect("failure") = Some("ferrite.toml: locked".to_owned());
    rig.press(cx, "f10");
    rig.click(cx, ("app-menu-tile", 1usize));
    *rig.store.failure.lock().expect("failure") = None;
    rig.click(cx, ("app-menu-tile", 1usize));
    assert_eq!(error_lines(cx, &rig), (None, None));
}
