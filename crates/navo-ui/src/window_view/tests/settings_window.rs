use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext};

use super::super::find_window::test_raw_handle;
use super::super::{FindHooks, NavoWindow, RawWindowHandle, SaveReport};
use super::{fake, open_options, options_for, settle, two_panes};
use crate::settings::{Change, FIELDS, Field};
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
