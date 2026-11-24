//! Settings editing copy: the named status line, its Undo button and key recording.

use gpui_kit::{AppContext as _, TestAppContext};

use super::app_menu::{Rig, rig};
use super::settings_sheet::{go_to, label, last_change, open, sheet_has_focus, shows};
use crate::settings::{Change, Field};
use crate::tokens::{Density, ThemeChoice};

/// Opens Settings and starts editing the Show Navo key; its row is listed only on Windows.
fn record_summon_key(cx: &mut TestAppContext, rig: &Rig) {
    open(cx, rig);
    cx.update_window(rig.window, |_, window, cx| {
        rig.view
            .update(cx, |view, cx| view.start_edit(Field::SummonKey, window, cx));
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_change_and_its_undo_name_the_setting_in_the_status_line(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    assert_eq!(
        label(cx, &rig, "settings-status").as_deref(),
        Some("Density set to Compact · saved")
    );
    rig.press(cx, "ctrl-z");
    assert_eq!(
        label(cx, &rig, "settings-status").as_deref(),
        Some("Undid Density: back to Normal")
    );
    assert!(!shows(cx, &rig, "settings-status-undo"));
}

#[gpui_kit::test]
fn the_undo_button_in_the_status_line_restores_the_value(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.store
        .present
        .lock()
        .expect("present")
        .push(Field::Density);
    open(cx, &rig);
    go_to(cx, &rig, Field::Density);
    rig.press(cx, "left");
    rig.click(cx, "settings-status-undo");
    assert_eq!(last_change(&rig), Some(Change::density(Density::Normal)));
}

#[gpui_kit::test]
fn recording_saves_the_next_key_combo_as_the_show_navo_key(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    record_summon_key(cx, &rig);
    assert!(shows(cx, &rig, "settings-record"));
    rig.press(cx, "k");
    assert!(label(cx, &rig, "settings-edit-error").is_some());
    rig.press(cx, "ctrl-alt-k");
    assert_eq!(
        last_change(&rig),
        Some(Change::text(Field::SummonKey, "Ctrl+Alt+K"))
    );
    assert!(!shows(cx, &rig, "settings-record"));
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn escape_stops_key_recording_without_a_change(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    record_summon_key(cx, &rig);
    rig.press(cx, "escape");
    assert!(!shows(cx, &rig, "settings-record"));
    assert!(sheet_has_focus(cx, &rig));
    assert!(rig.store.changes.lock().expect("changes").is_empty());
}
