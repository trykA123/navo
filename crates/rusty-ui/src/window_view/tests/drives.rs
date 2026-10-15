use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use rusty_core::{Command, PaneId};

use super::drive_fs::{Rig, Setup, drive_window, local};
use super::press;

fn bar(pane: PaneId) -> (&'static str, u64) {
    ("drives", pane.get())
}

fn first_pane(rig: &Rig, cx: &mut TestAppContext) -> PaneId {
    rig.panes(cx)[0]
}

#[gpui_kit::test]
fn the_drive_bar_shows_every_drive_and_marks_the_current_one(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    rig.frame(cx, |window, _| {
        let drives = window.within(bar(pane));
        for (index, name) in ["C", "D", "Z"].into_iter().enumerate() {
            assert_eq!(drives.find(("drive", index)).label(), Some(name));
        }
        assert!(drives.try_find(("drive", 3usize)).is_none());
        assert_eq!(drives.find(("drive", 0usize)).selected(), Some(true));
        assert_eq!(drives.find(("drive", 1usize)).selected(), Some(false));
        assert_eq!(drives.find(("drive", 0usize)).value(), Some("connected"));
        assert_eq!(drives.find(("drive", 2usize)).value(), Some("disconnected"));
    });
}

#[gpui_kit::test]
fn a_drive_click_opens_its_root_then_its_last_folder(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    rig.click(cx, bar(pane), ("drive", 1));
    assert_eq!(rig.location(cx, pane), local(r"D:\"));
    rig.view.update(cx, |view, _| {
        let location = local(r"D:\work");
        let _ = view.browser.command(&Command::PaneNavigate { location });
    });
    super::settle(cx, &rig.view);
    rig.click(cx, bar(pane), ("drive", 0));
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    rig.click(cx, bar(pane), ("drive", 1));
    assert_eq!(rig.location(cx, pane), local(r"D:\work"));
    rig.frame(cx, |window, _| {
        let drives = window.within(bar(pane));
        assert_eq!(drives.find(("drive", 1usize)).selected(), Some(true));
    });
}

#[gpui_kit::test]
fn a_disconnected_drive_reconnects_once_then_opens(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    rig.click(cx, bar(pane), ("drive", 2));
    let calls = rig.calls.lock().expect("calls").clone();
    assert_eq!(calls, [('Z', r"\\nas\share".to_owned())]);
    assert_eq!(rig.location(cx, pane), local(r"Z:\"));
}

#[gpui_kit::test]
fn a_failed_reconnect_shows_its_message_in_the_pane(cx: &mut TestAppContext) {
    let setup = Setup {
        reconnect: Err("The network path was not found.".to_owned()),
        ..Setup::default()
    };
    let rig = drive_window(cx, setup);
    let pane = first_pane(&rig, cx);
    rig.click(cx, bar(pane), ("drive", 2));
    assert_eq!(rig.calls.lock().expect("calls").len(), 1);
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    rig.frame(cx, |window, _| {
        let error = window.find(("pane-error", pane.get()));
        assert_eq!(error.label(), Some("The network path was not found."));
    });
}

fn menu_pane(rig: &Rig, cx: &mut TestAppContext) -> Option<PaneId> {
    rig.view
        .read_with(cx, |view, _| view.drive_menu.as_ref().map(|menu| menu.pane))
}

#[gpui_kit::test]
fn alt_f1_opens_the_menu_and_a_letter_then_enter_opens_that_drive(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let panes = rig.panes(cx);
    press(cx, rig.window, &rig.view, "alt-f1");
    assert_eq!(menu_pane(&rig, cx), Some(panes[0]));
    rig.frame(cx, |window, _| {
        let menu = window.within(("drive-menu", panes[0].get()));
        assert_eq!(menu.find(("drive-row", 0usize)).selected(), Some(true));
    });
    press(cx, rig.window, &rig.view, "d");
    press(cx, rig.window, &rig.view, "enter");
    assert_eq!(menu_pane(&rig, cx), None);
    assert_eq!(rig.location(cx, panes[0]), local(r"D:\"));
    assert_eq!(rig.location(cx, panes[1]), local(r"C:\home"));
}

#[gpui_kit::test]
fn alt_f2_targets_the_second_pane_and_escape_closes_without_moving(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let panes = rig.panes(cx);
    press(cx, rig.window, &rig.view, "alt-f2");
    assert_eq!(menu_pane(&rig, cx), Some(panes[1]));
    press(cx, rig.window, &rig.view, "down");
    press(cx, rig.window, &rig.view, "escape");
    assert_eq!(menu_pane(&rig, cx), None);
    for pane in panes {
        assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    }
}
