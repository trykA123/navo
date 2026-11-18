use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext};
use navo_core::Command;

use super::drive_fs::{Rig, Setup, drive_window, local};
use super::{press, type_in};
use crate::window_view::path_edit::Checked;

fn editing(rig: &Rig, cx: &mut TestAppContext) -> bool {
    rig.view.read_with(cx, |view, _| view.path_edit.is_some())
}

fn type_path(rig: &Rig, cx: &mut TestAppContext, text: &str) {
    press(cx, rig.window, &rig.view, "alt-d");
    assert!(editing(rig, cx));
    type_in(cx, rig.window, &rig.view, text);
    press(cx, rig.window, &rig.view, "enter");
}

#[gpui_kit::test]
fn alt_d_opens_the_path_input_on_the_current_path(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    press(cx, rig.window, &rig.view, "alt-d");
    assert!(editing(&rig, cx));
    rig.frame(cx, |window, _| {
        assert_eq!(window.find("path-field").value(), Some(r"C:\home"));
    });
    press(cx, rig.window, &rig.view, "escape");
    assert!(!editing(&rig, cx));
}

#[gpui_kit::test]
fn a_typed_folder_opens_it(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = rig.focused(cx);
    type_path(&rig, cx, r"C:\home\docs");
    assert!(!editing(&rig, cx));
    assert_eq!(rig.location(cx, pane), local(r"C:\home\docs"));
}

#[gpui_kit::test]
fn a_typed_file_opens_its_folder_on_that_file(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = rig.focused(cx);
    rig.view.update(cx, |view, _| {
        let location = local(r"C:\home\docs");
        let _ = view.browser.command(&Command::PaneNavigate { location });
    });
    super::settle(cx, &rig.view);
    type_path(&rig, cx, r"..\a.txt");
    assert!(!editing(&rig, cx));
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    let cursor = rig.view.read_with(cx, |view, _| {
        view.browser.state().active_tab().view().cursor
    });
    assert_eq!(cursor, 1);
}

#[gpui_kit::test]
fn a_bad_path_keeps_the_input_open_with_its_error(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    type_path(&rig, cx, "%NOPE%");
    assert!(editing(&rig, cx));
    rig.frame(cx, |window, _| {
        assert_eq!(
            window.find("path-error").label(),
            Some("Unknown variable %NOPE%.")
        );
    });
    press(cx, rig.window, &rig.view, "escape");
    assert!(!editing(&rig, cx));
}

#[gpui_kit::test]
fn a_missing_path_says_not_found(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = rig.focused(cx);
    type_path(&rig, cx, r"C:\home\nothing");
    assert!(editing(&rig, cx));
    rig.frame(cx, |window, _| {
        assert_eq!(window.find("path-error").label(), Some("Not found"));
    });
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
}

#[gpui_kit::test]
fn a_stalled_folder_says_it_is_not_answering_and_escape_goes_back(cx: &mut TestAppContext) {
    let setup = Setup {
        slow: Some("D/slow"),
        ..Setup::default()
    };
    let rig = drive_window(cx, setup);
    let pane = rig.focused(cx);
    rig.view.update(cx, |view, cx| {
        let location = local(r"D:\slow");
        let _ = view.browser.command(&Command::PaneNavigate { location });
        cx.notify();
    });
    cx.run_until_parked();
    let waiting = ("waiting", pane.get());
    assert!(rig.frame(cx, |window, _| window.try_find(waiting).is_none()));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    rig.frame(cx, |window, cx| {
        assert_eq!(
            window.find(waiting).label(),
            Some(r"D:\slow is not answering · retrying")
        );
        assert!(window.try_find(("retry", pane.get())).is_some());
        window.press("escape", cx);
    });
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some()
    });
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    assert!(rig.frame(cx, |window, _| window.try_find(waiting).is_none()));
    rig.fs.release();
}

fn crumbs_shown(rig: &Rig, cx: &mut TestAppContext) -> bool {
    rig.frame(cx, |window, _| window.try_find("path-field").is_none())
}

fn activate(rig: &Rig, cx: &mut TestAppContext) {
    cx.update_window(rig.window, |_, window, _| window.activate_window())
        .expect("window");
    rig.frame(cx, |_, _| ());
}

fn move_focus_off_the_field(rig: &Rig, cx: &mut TestAppContext) {
    cx.update_window(rig.window, |_, window, cx| {
        rig.view
            .update(cx, |view, cx| view.focus_core_pane(window, cx));
    })
    .expect("window");
    super::settle(cx, &rig.view);
    rig.frame(cx, |_, _| ());
}

#[gpui_kit::test]
fn focus_leaving_the_path_field_closes_it_and_brings_the_crumbs_back(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    activate(&rig, cx);
    press(cx, rig.window, &rig.view, "alt-d");
    assert!(!crumbs_shown(&rig, cx));
    move_focus_off_the_field(&rig, cx);
    assert!(!editing(&rig, cx));
    assert!(crumbs_shown(&rig, cx));
}

#[gpui_kit::test]
fn navigating_back_closes_the_path_input(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    rig.view.update(cx, |view, _| {
        let location = local(r"C:\home\docs");
        let _ = view.browser.command(&Command::PaneNavigate { location });
    });
    super::settle(cx, &rig.view);
    press(cx, rig.window, &rig.view, "alt-d");
    assert!(editing(&rig, cx));
    rig.view.update(cx, |view, cx| {
        let _ = view.browser.command(&Command::PaneBack);
        cx.notify();
    });
    super::settle(cx, &rig.view);
    assert!(!editing(&rig, cx));
    assert!(crumbs_shown(&rig, cx));
}

#[gpui_kit::test]
fn switching_tab_closes_the_path_input(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    rig.view.update(cx, |view, cx| {
        let _ = view
            .browser
            .command(&Command::PaneOpenTab { location: None });
        cx.notify();
    });
    super::settle(cx, &rig.view);
    press(cx, rig.window, &rig.view, "alt-d");
    assert!(editing(&rig, cx));
    rig.view.update(cx, |view, cx| {
        let _ = view.browser.command(&Command::PaneActivateTab { tab: 0 });
        cx.notify();
    });
    super::settle(cx, &rig.view);
    assert!(!editing(&rig, cx));
}

#[gpui_kit::test]
fn the_window_losing_activation_keeps_the_path_input_open(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    activate(&rig, cx);
    press(cx, rig.window, &rig.view, "alt-d");
    let mut visual = gpui_kit::VisualTestContext::from_window(rig.window, cx);
    visual.deactivate_window();
    super::settle(cx, &rig.view);
    rig.frame(cx, |_, _| ());
    assert!(editing(&rig, cx));
}

#[gpui_kit::test]
fn the_path_field_menu_keeps_the_edit_until_focus_leaves_field_and_menu(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    activate(&rig, cx);
    press(cx, rig.window, &rig.view, "alt-d");
    let field = rig.frame(cx, |window, cx| window.focused(cx));
    rig.frame(cx, |window, cx| window.right_click("path-field", cx));
    super::settle(cx, &rig.view);
    let menu = rig.frame(cx, |window, cx| window.focused(cx));
    // Windows and macOS show an OS popup that leaves GPUI focus in the field;
    // other platforms draw a menu that takes focus.
    let is_native = cfg!(any(target_os = "windows", target_os = "macos"));
    assert!(menu.is_some(), "focus stays in the field or its menu");
    assert_eq!(menu == field, is_native, "only a drawn menu takes focus");
    rig.frame(cx, |_, _| ());
    assert!(editing(&rig, cx), "the menu is part of the field");
    move_focus_off_the_field(&rig, cx);
    assert!(!editing(&rig, cx));
}

/// The check answers after a click moved focus but before the frame that reports the blur.
#[gpui_kit::test]
fn a_path_check_that_ends_after_a_click_elsewhere_does_not_navigate(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    activate(&rig, cx);
    let pane = rig.focused(cx);
    let other = rig
        .panes(cx)
        .into_iter()
        .find(|id| *id != pane)
        .expect("two panes");
    press(cx, rig.window, &rig.view, "alt-d");
    type_in(cx, rig.window, &rig.view, r"C:\home\docs");
    cx.update_window(rig.window, |_, window, cx| window.press("enter", cx))
        .expect("window");
    let elsewhere = rig.view.read_with(cx, |view, _| view.focus[&other].clone());
    cx.update_window(rig.window, |_, window, cx| {
        window.focus(&elsewhere, cx);
        rig.view.update(cx, |view, cx| {
            let id = view.path_edit.as_ref().expect("editing").id;
            let location = local(r"C:\home\docs");
            let checked = Checked {
                id,
                location,
                found: None,
            };
            view.path_checked(checked, window, cx);
        });
    })
    .expect("window");
    super::settle(cx, &rig.view);
    assert!(rig.frame(cx, |window, _| elsewhere.is_focused(window)));
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    assert!(!editing(&rig, cx));
}

/// Opens the path edit, then lets the window draw one frame while inactive.
fn edit_then_deactivate(rig: &Rig, cx: &mut TestAppContext) {
    activate(rig, cx);
    press(cx, rig.window, &rig.view, "alt-d");
    rig.frame(cx, |_, _| ());
    gpui_kit::VisualTestContext::from_window(rig.window, cx).deactivate_window();
    super::settle(cx, &rig.view);
    rig.frame(cx, |_, _| ());
}

#[gpui_kit::test]
fn a_click_that_activates_the_window_on_another_pane_closes_the_path_input(
    cx: &mut TestAppContext,
) {
    let rig = drive_window(cx, Setup::default());
    let pane = rig.focused(cx);
    let other = rig.panes(cx).into_iter().find(|id| *id != pane);
    let elsewhere = rig
        .view
        .read_with(cx, |view, _| view.focus[&other.expect("two panes")].clone());
    edit_then_deactivate(&rig, cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.activate_window();
        window.focus(&elsewhere, cx);
    })
    .expect("window");
    super::settle(cx, &rig.view);
    assert!(rig.frame(cx, |window, _| elsewhere.is_focused(window)));
    assert!(!editing(&rig, cx));
}

#[gpui_kit::test]
fn coming_back_to_the_window_keeps_the_path_input(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    edit_then_deactivate(&rig, cx);
    activate(&rig, cx);
    super::settle(cx, &rig.view);
    rig.frame(cx, |_, _| ());
    assert!(editing(&rig, cx));
}
