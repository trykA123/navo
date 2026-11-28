use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, VisualTestContext,
};

use super::super::find_window::test_raw_handle;
use super::super::{FindHooks, NavoWindow, RawWindowHandle, SaveReport};
use super::{fake, open_options, options_for, settle, two_panes};
use crate::settings::{Change, FIELDS, Field, Section, Value};
use crate::tokens::Density;

type Saves = Arc<Mutex<Vec<Change>>>;
type OwnerCalls = Arc<Mutex<Vec<(String, String)>>>;

fn raw_tag(handle: RawWindowHandle) -> String {
    format!("{handle:?}")
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    saves: Saves,
    owner_calls: OwnerCalls,
}

fn rig(cx: &mut TestAppContext, owner_repaired: bool) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let mut options = options_for(fs, state);
    let saves: Saves = Arc::default();
    let recorded = saves.clone();
    let base = options.settings.values.clone();
    options.settings.save = Arc::new(move |changes| {
        recorded.lock().expect("saves").extend_from_slice(changes);
        let values = changes.iter().fold(base.clone(), |all, one| all.with(one));
        SaveReport::saved(values)
    });
    let owner_calls: OwnerCalls = Arc::default();
    let recorded_owners = owner_calls.clone();
    options.search.find = FindHooks {
        ensure_owner: Arc::new(move |settings_raw, navo_raw| {
            recorded_owners
                .lock()
                .expect("owner calls")
                .push((raw_tag(settings_raw), raw_tag(navo_raw)));
            owner_repaired
        }),
        ..FindHooks::none()
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        saves,
        owner_calls,
    }
}

fn open_settings(cx: &mut TestAppContext, rig: &Rig) {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| view.open_settings_window(window, cx));
    })
    .expect("navo window");
    cx.run_until_parked();
}

fn settings_window(rig: &Rig, cx: &TestAppContext) -> Option<AnyWindowHandle> {
    rig.view
        .read_with(cx, |view, _| view.settings.host.window_handle())
}

fn press_settings(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    press_without_parking(cx, handle, key);
    cx.run_until_parked();
}

fn press_without_parking(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
    })
    .expect("settings window");
}

#[gpui_kit::test]
fn opening_twice_keeps_one_window_and_focuses_the_existing(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    assert_eq!(cx.windows().len(), 1);

    open_settings(cx, &rig);
    assert_eq!(cx.windows().len(), 2);
    let first = settings_window(&rig, cx).expect("settings window handle");

    VisualTestContext::from_window(first, cx).deactivate_window();
    cx.run_until_parked();
    let active_before = cx
        .update_window(first, |_, window, _| window.is_window_active())
        .expect("settings window");
    assert!(!active_before);

    open_settings(cx, &rig);
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(settings_window(&rig, cx), Some(first));
    let active_after = cx
        .update_window(first, |_, window, _| window.is_window_active())
        .expect("settings window");
    assert!(active_after);
}

fn row_of(field: Field) -> usize {
    FIELDS
        .iter()
        .position(|spec| spec.field == field)
        .expect("every field has a row")
}

fn go_to(cx: &mut TestAppContext, rig: &Rig, handle: AnyWindowHandle, field: Field) {
    for _ in 0..FIELDS.len() * 2 {
        if rig.view.read_with(cx, |view, _| view.settings_row()) == row_of(field) {
            return;
        }
        press_settings(cx, handle, "down");
    }
    panic!("{field:?} is not reachable with the down key");
}

#[gpui_kit::test]
fn down_moves_the_row_and_right_changes_a_value_navo_keeps(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");
    let start = rig.view.read_with(cx, |view, _| view.settings_row());

    press_settings(cx, handle, "down");
    let moved = rig.view.read_with(cx, |view, _| view.settings_row());
    assert_ne!(start, moved, "the down key moves the focused row");

    go_to(cx, &rig, handle, Field::Density);
    let before = rig.view.read_with(cx, |view, _| view.values.density());
    press_settings(cx, handle, "right");
    let after = rig.view.read_with(cx, |view, _| view.values.density());
    assert_ne!(before, after);
    assert_eq!(after, Density::Comfortable);
    let saved = rig.saves.lock().expect("saves").clone();
    assert_eq!(saved.last(), Some(&Change::density(Density::Comfortable)));
}

#[gpui_kit::test]
fn a_save_settling_after_close_still_reaches_the_saver(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");

    // Change a value, then close the Settings window before the save settles.
    press_without_parking(cx, handle, "right");
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| view.close_settings_window(window, cx));
    })
    .expect("navo window");
    cx.run_until_parked();

    assert_eq!(cx.windows().len(), 1);
    assert_eq!(settings_window(&rig, cx), None);
    let saved = rig.saves.lock().expect("saves").clone();
    assert!(!saved.is_empty(), "the save never reached the saver");
    // The settle ran on Navo's window: a spawn on the dead Settings window would stay busy.
    let (busy, theme) = rig.view.read_with(cx, |view, _| {
        (view.saver.is_busy(), view.values.get(Field::Theme).clone())
    });
    assert!(!busy);
    let saved_theme = saved
        .iter()
        .find(|change| change.field == Field::Theme)
        .map(|change| change.value.clone());
    assert_eq!(saved_theme, Some(theme));
}

#[gpui_kit::test]
fn ensure_owner_hook_is_called_once_on_open_with_settings_and_navo_handles(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, true);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");

    let calls = rig.owner_calls.lock().expect("owner calls").clone();
    assert_eq!(
        calls,
        vec![(
            raw_tag(test_raw_handle(handle)),
            raw_tag(test_raw_handle(rig.window))
        )]
    );
    assert_ne!(calls[0].0, calls[0].1);

    open_settings(cx, &rig);
    assert_eq!(rig.owner_calls.lock().expect("owner calls").len(), 1);
}

#[gpui_kit::test]
fn ctrl_k_inside_settings_opens_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");

    press_settings(cx, handle, "ctrl-k");
    assert!(
        !rig.view
            .read_with(cx, |view, _| view.command_center.is_open())
    );
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(settings_window(&rig, cx), Some(handle));
}

#[gpui_kit::test]
fn rapid_double_open_before_defer_keeps_one_settings_window(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    assert_eq!(cx.windows().len(), 1);

    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.open_settings_window(window, cx);
            view.open_settings_window(window, cx);
        });
    })
    .expect("navo window");
    cx.run_until_parked();

    assert_eq!(cx.windows().len(), 2);
    assert!(settings_window(&rig, cx).is_some());
}

#[gpui_kit::test]
fn ctrl_comma_inside_settings_closes_the_window_and_clears_host(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");
    assert_eq!(cx.windows().len(), 2);

    press_settings(cx, handle, "ctrl-,");

    assert!(
        !rig.view
            .read_with(cx, |view, _| view.settings.host.is_open())
    );
    assert_eq!(cx.windows().len(), 1);
}

fn shows_in(cx: &mut TestAppContext, handle: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("settings window")
}

fn shows_row_in(cx: &mut TestAppContext, handle: AnyWindowHandle, field: Field) -> bool {
    shows_in(cx, handle, ("settings-row", row_of(field)))
}

fn click_settings(cx: &mut TestAppContext, handle: AnyWindowHandle, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .expect("settings window");
    cx.run_until_parked();
}

fn type_settings(cx: &mut TestAppContext, handle: AnyWindowHandle, text: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.input(text, cx);
    })
    .expect("settings window");
    cx.run_until_parked();
}

fn find_text_in(cx: &mut TestAppContext, handle: AnyWindowHandle) -> Option<String> {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.find("settings-find").value().map(str::to_owned)
    })
    .expect("settings window")
}

#[gpui_kit::test]
fn find_filters_rows_inside_settings_window_and_escape_clears_before_closing(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");
    assert!(shows_in(cx, handle, "settings-find"));

    press_settings(cx, handle, "ctrl-f");
    type_settings(cx, handle, "git");
    assert!(shows_row_in(cx, handle, Field::GitBadges));
    assert!(!shows_row_in(cx, handle, Field::TerminalProvider));
    assert!(!shows_row_in(cx, handle, Field::Theme));

    press_settings(cx, handle, "escape");
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(settings_window(&rig, cx), Some(handle));
    assert_eq!(find_text_in(cx, handle).as_deref(), Some(""));
    assert!(shows_row_in(cx, handle, Field::TerminalProvider));

    press_settings(cx, handle, "escape");
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(settings_window(&rig, cx), None);
}

#[gpui_kit::test]
fn enter_in_typed_edit_inside_settings_window_saves_and_updates_values(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");

    go_to(cx, &rig, handle, Field::Font);
    press_settings(cx, handle, "enter");
    assert!(shows_in(cx, handle, "settings-input"));

    press_settings(cx, handle, "ctrl-a");
    type_settings(cx, handle, "Archivo");
    press_settings(cx, handle, "enter");

    assert!(!shows_in(cx, handle, "settings-input"));
    let saved = rig.saves.lock().expect("saves").clone();
    assert_eq!(saved.last(), Some(&Change::text(Field::Font, "Archivo")));
    let value = rig
        .view
        .read_with(cx, |view, _| view.values.get(Field::Font).clone());
    assert_eq!(value, Value::Text("Archivo".into()));
    assert!(!rig.view.read_with(cx, |view, _| view.saver.is_busy()));
}

#[gpui_kit::test]
fn keybinding_recording_inside_settings_window_stops_propagation_and_saves_combo(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");

    // Field::SummonKey is Windows-only in Filter, so start recording directly as in settings_status.
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| view.start_edit(Field::SummonKey, window, cx));
    })
    .expect("navo window");
    cx.run_until_parked();
    assert!(shows_in(cx, handle, "settings-record"));

    // Unmodified key is captured by on_key_down (with synchronous stop_propagation) and refused.
    press_settings(cx, handle, "k");
    assert_eq!(cx.windows().len(), 2);
    assert!(shows_in(cx, handle, "settings-edit-error"));
    assert!(shows_in(cx, handle, "settings-record"));

    press_settings(cx, handle, "ctrl-alt-k");
    assert!(!shows_in(cx, handle, "settings-record"));
    let saved = rig.saves.lock().expect("saves").clone();
    assert_eq!(
        saved.last(),
        Some(&Change::text(Field::SummonKey, "Ctrl+Alt+K"))
    );
    let value = rig
        .view
        .read_with(cx, |view, _| view.values.get(Field::SummonKey).clone());
    assert_eq!(value, Value::Text("Ctrl+Alt+K".into()));

    // CancelEdit action on SettingsField cancels recording without closing SettingsWindow.
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| view.start_edit(Field::SummonKey, window, cx));
    })
    .expect("navo window");
    cx.run_until_parked();
    assert!(shows_in(cx, handle, "settings-record"));
    press_settings(cx, handle, "escape");
    assert!(!shows_in(cx, handle, "settings-record"));
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(settings_window(&rig, cx), Some(handle));
}

#[gpui_kit::test]
fn closing_and_reopening_settings_window_recreates_find_bound_to_new_window(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let first = settings_window(&rig, cx).expect("first settings window");

    press_settings(cx, first, "ctrl-f");
    type_settings(cx, first, "git");
    assert!(shows_row_in(cx, first, Field::GitBadges));
    assert!(!shows_row_in(cx, first, Field::TerminalProvider));

    press_settings(cx, first, "ctrl-,");
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(settings_window(&rig, cx), None);

    open_settings(cx, &rig);
    let second = settings_window(&rig, cx).expect("reopened settings window");
    assert_ne!(second, first);
    assert_eq!(find_text_in(cx, second).as_deref(), Some(""));
    assert!(shows_row_in(cx, second, Field::TerminalProvider));

    press_settings(cx, second, "ctrl-f");
    type_settings(cx, second, "appearance.density");
    assert!(shows_row_in(cx, second, Field::Density));
    assert!(!shows_row_in(cx, second, Field::GitBadges));
}

#[gpui_kit::test]
fn shortcuts_page_inside_settings_window_records_with_enter_and_cancels_with_escape(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx, false);
    open_settings(cx, &rig);
    let handle = settings_window(&rig, cx).expect("settings window handle");

    let keyboard = Section::ALL
        .iter()
        .position(|page| *page == Section::Keyboard)
        .expect("Keyboard section");
    click_settings(cx, handle, ("settings-section", keyboard));
    click_settings(cx, handle, "settings-keyboard-shortcuts");

    assert!(
        rig.view
            .read_with(cx, |view, _| view.is_showing_shortcuts())
    );
    assert!(shows_in(cx, handle, "shortcuts-page"));
    let page = rig
        .view
        .read_with(cx, |view, _| view.shortcuts_page())
        .expect("shortcuts page");

    press_settings(cx, handle, "enter");
    assert!(page.read_with(cx, |page, _| page.is_recording()));

    press_settings(cx, handle, "escape");
    assert!(!page.read_with(cx, |page, _| page.is_recording()));
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(settings_window(&rig, cx), Some(handle));

    press_settings(cx, handle, "escape");
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(settings_window(&rig, cx), None);
}

#[gpui_kit::test]
fn ctrl_comma_twice_in_navo_keeps_one_focused_settings_window(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    assert_eq!(cx.windows().len(), 1);

    super::press(cx, rig.window, &rig.view, "ctrl-,");
    assert_eq!(cx.windows().len(), 2);
    let first = settings_window(&rig, cx).expect("ctrl-, opens a Settings window");
    assert!(rig.view.read_with(cx, |view, _| view.settings.is_open()));

    VisualTestContext::from_window(first, cx).deactivate_window();
    cx.run_until_parked();
    let active_before = cx
        .update_window(first, |_, window, _| window.is_window_active())
        .expect("settings window");
    assert!(!active_before);

    super::press(cx, rig.window, &rig.view, "ctrl-,");
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(settings_window(&rig, cx), Some(first));
    assert!(rig.view.read_with(cx, |view, _| view.settings.is_open()));
    let active_after = cx
        .update_window(first, |_, window, _| window.is_window_active())
        .expect("settings window");
    assert!(active_after, "second Ctrl+, in Navo focuses Settings");
}

#[gpui_kit::test]
fn preview_stays_open_across_opening_and_closing_settings(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    super::press(cx, rig.window, &rig.view, "f3");
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(rig.view.read_with(cx, |view, _| view.preview.is_open()));
    assert!(shows_in(cx, rig.window, "preview-sheet"));

    super::press(cx, rig.window, &rig.view, "ctrl-,");
    let handle = settings_window(&rig, cx).expect("ctrl-, opens a Settings window");
    assert!(
        rig.view.read_with(cx, |view, _| view.preview.is_open()),
        "opening Settings leaves preview open"
    );
    assert!(
        shows_in(cx, rig.window, "preview-sheet"),
        "preview stays drawn in Navo"
    );

    press_settings(cx, handle, "escape");
    assert_eq!(settings_window(&rig, cx), None);
    assert!(
        rig.view.read_with(cx, |view, _| view.preview.is_open()),
        "closing Settings leaves preview open"
    );
    assert!(shows_in(cx, rig.window, "preview-sheet"));
}

#[gpui_kit::test]
fn closing_settings_over_a_compare_file_diff_focuses_the_diff(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = super::compare_screen::rig(cx);
    rig.press(cx, "enter");
    let file_focused = |cx: &mut TestAppContext| {
        super::screen_access::screen_update_in(&rig.view, rig.window, cx, |view, window, _| {
            view.file_screen_focused(window)
        })
    };
    assert!(file_focused(cx), "file diff starts focused");

    rig.press(cx, "ctrl-,");
    let handle = rig
        .view
        .read_with(cx, |view, _| view.settings.host.window_handle())
        .expect("ctrl-, opens a Settings window");
    // Move focus away from the diff in Navo so closing Settings must actively refocus it.
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.update(cx, |view, cx| view.focus_core_pane(window, cx));
    })
    .expect("navo window");
    assert!(!file_focused(cx), "diff focus was moved away before close");

    press_settings(cx, handle, "escape");
    assert!(
        !rig.view.read_with(cx, |view, _| view.settings.is_open()),
        "Settings closed"
    );
    assert_eq!(
        rig.view
            .read_with(cx, |view, _| view.settings.host.window_handle()),
        None
    );
    assert!(
        file_focused(cx),
        "closing Settings returns focus to the compare file diff"
    );
}
