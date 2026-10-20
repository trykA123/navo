use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Modifiers, Pixels, TestAppContext,
};
use rusty_core::AppState;
use rusty_fs::FakeFs;

use super::app_menu::{Rig, rig_on};
use super::fkeys::{hold, key_labels};
use super::{FerriteWindow, fake, open, open_with, press, two_panes, with_fkey_bar};
use crate::commands::{Keymap, set_active};
use crate::modifier_hints::HintsMode;
use crate::settings::{Change, FIELDS, Field, Values};

const CTRL: Modifiers = Modifiers {
    control: true,
    alt: false,
    shift: false,
    platform: false,
    function: false,
};

const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..CTRL
};

fn wait(cx: &mut TestAppContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn drawer(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("modifier-hints")
            .map(|drawer| drawer.label().unwrap_or_default().to_owned())
    })
    .expect("window")
}

fn has_entry(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("modifier-hints")
            .try_find(id.to_owned())
            .is_some()
    })
    .expect("window")
}

fn hold_open(cx: &mut TestAppContext, window: AnyWindowHandle) {
    hold(cx, window, CTRL);
    wait(cx, 400);
}

fn rig_with_hints(cx: &mut TestAppContext, hints: HintsMode) -> Rig {
    rig_on(
        cx,
        with_fkey_bar(Values::default()).with(&Change::modifier_hints(hints)),
        None,
    )
}

#[gpui_kit::test]
fn holding_ctrl_in_the_file_list_opens_the_drawer_after_the_delay(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    hold(cx, window, CTRL);
    assert_eq!(drawer(cx, window), None);
    wait(cx, 399);
    assert_eq!(drawer(cx, window), None);
    wait(cx, 1);
    let label = drawer(cx, window).expect("the drawer is open");
    assert!(label.contains("Ctrl"), "{label}");
    assert!(has_entry(cx, window, "hint-files.clipboard_copy"));
    assert!(has_entry(cx, window, "hint-more-preview.browse"));
}

#[gpui_kit::test]
fn ctrl_c_never_opens_the_drawer(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    hold(cx, window, CTRL);
    press(cx, window, &view, "ctrl-c");
    wait(cx, 2000);
    assert_eq!(drawer(cx, window), None);
    hold(cx, window, CTRL_SHIFT);
    wait(cx, 2000);
    assert_eq!(drawer(cx, window), None);
    hold(cx, window, Modifiers::default());
    hold_open(cx, window);
    assert!(drawer(cx, window).is_some());
}

#[gpui_kit::test]
fn releasing_every_modifier_hides_the_drawer(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    hold_open(cx, window);
    assert!(drawer(cx, window).is_some());
    hold(cx, window, Modifiers::default());
    assert_eq!(drawer(cx, window), None);
}

#[gpui_kit::test]
fn adding_shift_switches_the_drawer_to_ctrl_shift_at_once(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    hold_open(cx, window);
    hold(cx, window, CTRL_SHIFT);
    let label = drawer(cx, window).expect("still open");
    assert!(label.contains("Ctrl+Shift"), "{label}");
    assert!(has_entry(cx, window, "hint-pane.reset_split"));
}

#[gpui_kit::test]
fn a_rebind_shows_in_the_drawer_at_once(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    let text = "[[bind]]\ncommand = \"files.refresh\"\nkeys = [\"ctrl-shift-f9\"]\n";
    let keymap = Keymap::builtin().overlay(text).expect("keymap").keymap;
    cx.update(|cx| set_active(cx, keymap));
    hold_open(cx, window);
    assert!(!has_entry(cx, window, "hint-files.refresh"));
    assert!(has_entry(cx, window, "hint-more-files.refresh"));
}

#[gpui_kit::test]
fn the_viewer_never_opens_the_drawer(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    let file = fs.location("home/a.txt");
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.view_location(file, window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.viewer.is_some()));
    hold_open(cx, window);
    assert_eq!(drawer(cx, window), None);
}

#[gpui_kit::test]
fn fkeys_mode_switches_the_f_key_labels_without_a_drawer(cx: &mut TestAppContext) {
    let rig = rig_with_hints(cx, HintsMode::Fkeys);
    hold_open(cx, rig.window);
    assert_eq!(drawer(cx, rig.window), None);
    let (labels, lit) = key_labels(cx, rig.window);
    assert_eq!(labels[2], "F3 By name");
    assert_eq!(lit[1], Some(true));
}

#[gpui_kit::test]
fn off_keeps_the_f_key_bar_fixed_and_opens_no_drawer(cx: &mut TestAppContext) {
    let rig = rig_with_hints(cx, HintsMode::Off);
    let (base, _) = key_labels(cx, rig.window);
    hold_open(cx, rig.window);
    assert_eq!(drawer(cx, rig.window), None);
    let (labels, lit) = key_labels(cx, rig.window);
    assert_eq!(labels, base);
    assert_eq!(lit, [Some(false); 3]);
}

#[gpui_kit::test]
fn turn_off_keeps_the_f_keys_and_opens_settings_on_that_row(cx: &mut TestAppContext) {
    let rig = rig_with_hints(cx, HintsMode::All);
    hold_open(cx, rig.window);
    rig.click(cx, "modifier-hints-off");
    wait(cx, 1000);
    let values = rig.view.read_with(cx, |view, _| view.values.clone());
    assert_eq!(values.modifier_hints(), HintsMode::Fkeys);
    assert!(rig.view.read_with(cx, |view, _| view.settings.is_open()));
    let row = rig.view.read_with(cx, |view, _| view.settings_row());
    assert_eq!(FIELDS[row].field, Field::ModifierHints);
    assert_eq!(drawer(cx, rig.window), None);
}

fn many_files() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for index in 0..60 {
        fs.add_file(&format!("home/file-{index:02}.txt"), 1);
    }
    fs.add_folder("dest");
    Arc::new(fs)
}

struct Shot {
    row: Bounds<Pixels>,
    scroll: u64,
    drawer_top: Option<Pixels>,
}

fn cursor_row(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> Shot {
    let (pane, view_state) = view.read_with(cx, |view, _| {
        let state = view.browser.state();
        (state.focused_pane(), state.active_tab().view())
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        let drawer_top = window
            .try_find("modifier-hints")
            .map(|drawer| drawer.bounds().top());
        let cursor = usize::try_from(view_state.cursor).expect("cursor");
        let row = window
            .within(("list", pane.get()))
            .find(("row", cursor))
            .bounds();
        Shot {
            row,
            scroll: view_state.scroll,
            drawer_top,
        }
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_low_cursor_row_moves_above_the_drawer_and_back(cx: &mut TestAppContext) {
    let fs = many_files();
    let state: AppState = two_panes(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "end");
    let before = cursor_row(cx, window, &view);
    hold_open(cx, window);
    let open = cursor_row(cx, window, &view);
    let top = open.drawer_top.expect("the drawer is open");
    assert!(before.row.bottom() > top, "the test row was not covered");
    assert!(open.row.bottom() <= top, "{:?} under {top:?}", open.row);
    assert_eq!(open.scroll, before.scroll);
    hold(cx, window, Modifiers::default());
    let after = cursor_row(cx, window, &view);
    assert_eq!(after.row, before.row);
    assert_eq!(after.scroll, before.scroll);
}

#[gpui_kit::test]
fn commands_without_a_handler_are_left_out_of_the_drawer(cx: &mut TestAppContext) {
    let (window, _) = open(cx, Rc::default());
    hold_open(cx, window);
    assert!(has_entry(cx, window, "hint-files.refresh"));
    assert!(has_entry(cx, window, "hint-tab.go_to_first"));
    assert!(!has_entry(cx, window, "hint-window.new"));
}
