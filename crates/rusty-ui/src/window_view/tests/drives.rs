use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{TestAppContext, VisualTestContext};
use rusty_core::{Command, PaneId};

use super::drive_fs::{Rig, Setup, drive_window, local};
use super::press;

fn pill(pane: PaneId) -> (&'static str, u64) {
    ("drive-pill", pane.get())
}

fn open_list(rig: &Rig, cx: &mut TestAppContext, pane: PaneId) {
    rig.frame(cx, |window, cx| window.click(pill(pane), cx));
    super::settle(cx, &rig.view);
}

fn pick(rig: &Rig, cx: &mut TestAppContext, pane: PaneId, index: usize) {
    open_list(rig, cx, pane);
    rig.click(cx, ("drive-menu", pane.get()), ("drive-row", index));
}

fn first_pane(rig: &Rig, cx: &mut TestAppContext) -> PaneId {
    rig.panes(cx)[0]
}

#[gpui_kit::test]
fn the_drive_pill_names_the_current_drive_and_lists_every_drive(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    rig.frame(cx, |window, _| {
        let current = window.find(pill(pane));
        assert_eq!(current.label(), Some("Drive C:"));
        assert_eq!(current.value(), Some("connected"));
        assert_eq!(current.expanded(), Some(false));
    });
    open_list(&rig, cx, pane);
    rig.frame(cx, |window, _| {
        assert_eq!(window.find(pill(pane)).expanded(), Some(true));
        let drives = window.within(("drive-menu", pane.get()));
        for (index, name) in ["C", "D", "Z"].into_iter().enumerate() {
            assert_eq!(drives.find(("drive-row", index)).label(), Some(name));
        }
        assert!(drives.try_find(("drive-row", 3usize)).is_none());
        assert_eq!(drives.find(("drive-row", 0usize)).selected(), Some(true));
        assert_eq!(drives.find(("drive-row", 1usize)).selected(), Some(false));
        assert_eq!(
            drives.find(("drive-row", 0usize)).value(),
            Some("connected")
        );
        assert_eq!(
            drives.find(("drive-row", 2usize)).value(),
            Some("disconnected")
        );
    });
}

#[gpui_kit::test]
fn a_drive_click_opens_its_root_then_its_last_folder(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    pick(&rig, cx, pane, 1);
    assert_eq!(rig.location(cx, pane), local(r"D:\"));
    rig.view.update(cx, |view, _| {
        let location = local(r"D:\work");
        let _ = view.browser.command(&Command::PaneNavigate { location });
    });
    super::settle(cx, &rig.view);
    pick(&rig, cx, pane, 0);
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    pick(&rig, cx, pane, 1);
    assert_eq!(rig.location(cx, pane), local(r"D:\work"));
    rig.frame(cx, |window, _| {
        assert_eq!(window.find(pill(pane)).label(), Some("Drive D:"));
    });
}

#[gpui_kit::test]
fn a_disconnected_drive_reconnects_once_then_opens(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    pick(&rig, cx, pane, 2);
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
    pick(&rig, cx, pane, 2);
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

#[gpui_kit::test]
fn a_reconnect_error_is_gone_after_leaving_and_coming_back(cx: &mut TestAppContext) {
    let setup = Setup {
        reconnect: Err("The network path was not found.".to_owned()),
        ..Setup::default()
    };
    let rig = drive_window(cx, setup);
    let pane = first_pane(&rig, cx);
    pick(&rig, cx, pane, 2);
    pick(&rig, cx, pane, 1);
    pick(&rig, cx, pane, 0);
    assert_eq!(rig.location(cx, pane), local(r"C:\home"));
    rig.frame(cx, |window, _| {
        assert!(window.try_find(("pane-error", pane.get())).is_none());
    });
}

#[gpui_kit::test]
fn clicking_the_pill_of_an_open_list_closes_it(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    open_list(&rig, cx, pane);
    assert_eq!(menu_pane(&rig, cx), Some(pane));
    open_list(&rig, cx, pane);
    assert_eq!(menu_pane(&rig, cx), None);
    open_list(&rig, cx, pane);
    assert_eq!(menu_pane(&rig, cx), Some(pane));
}

fn idle(cx: &mut TestAppContext, wait: Duration) {
    cx.executor().advance_clock(wait);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn drives_are_read_once_at_start_and_not_again_while_idle(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let started = rig.reads();
    assert!(started >= 1, "the drive bar needs one read at start");
    idle(cx, Duration::from_secs(30));
    assert_eq!(rig.reads(), started);
}

#[gpui_kit::test]
fn window_activation_reads_the_drives_once(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let mut visual = VisualTestContext::from_window(rig.window, cx);
    visual.deactivate_window();
    visual.run_until_parked();
    let before = rig.reads();
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert_eq!(rig.reads(), before + 1);
}

#[gpui_kit::test]
fn a_burst_of_volume_changes_reads_the_drives_once_after_they_settle(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let before = rig.reads();
    for _ in 0..3 {
        rig.signal.send();
    }
    idle(cx, Duration::from_millis(100));
    assert_eq!(rig.reads(), before);
    idle(cx, Duration::from_millis(300));
    assert_eq!(rig.reads(), before + 1);
    idle(cx, Duration::from_secs(5));
    assert_eq!(rig.reads(), before + 1);
}

#[gpui_kit::test]
fn opening_the_drive_list_reads_the_drives(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let pane = first_pane(&rig, cx);
    let before = rig.reads();
    open_list(&rig, cx, pane);
    assert_eq!(rig.reads(), before + 1);
}

#[gpui_kit::test]
fn staggered_volume_changes_restart_the_settle_deadline(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let before = rig.reads();
    rig.signal.send();
    cx.run_until_parked();
    idle(cx, Duration::from_millis(290));
    rig.signal.send();
    cx.run_until_parked();
    idle(cx, Duration::from_millis(10));
    assert_eq!(rig.reads(), before);
    idle(cx, Duration::from_millis(289));
    assert_eq!(rig.reads(), before);
    idle(cx, Duration::from_millis(1));
    assert_eq!(rig.reads(), before + 1);
}

#[gpui_kit::test]
fn continuous_volume_changes_refresh_within_two_seconds(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let before = rig.reads();
    let start = cx.background_executor.now();
    rig.signal.send();
    cx.run_until_parked();
    for _ in 0..6 {
        idle(cx, Duration::from_millis(290));
        rig.signal.send();
        cx.run_until_parked();
        assert_eq!(rig.reads(), before);
    }
    idle(cx, Duration::from_millis(259));
    assert_eq!(rig.reads(), before);
    idle(cx, Duration::from_millis(1));
    assert_eq!(cx.background_executor.now() - start, Duration::from_secs(2));
    assert_eq!(rig.reads(), before + 1);
}
