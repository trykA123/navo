use std::time::Duration;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use rusty_core::Command;

use super::drive_fs::{Rig, Setup, drive_window, local};
use super::{press, type_in};

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
