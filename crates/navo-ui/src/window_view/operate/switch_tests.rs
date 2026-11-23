//! AD11c: the copy and move dialog shows the "Verify after copy" switch with the default the
//! browser resolved, hides it for other jobs, and a click flips the choice.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_fs::DriveClass;

use crate::window_view::NavoWindow;
use crate::window_view::tests::{fake, home_beside_dest, open_options, options_for, press, settle};

fn removable(_: &navo_core::Location) -> DriveClass {
    DriveClass::Removable
}

/// Opens a window whose drives all read as removable and presses `keys` in order.
fn dialog_after(cx: &mut TestAppContext, keys: &[&str]) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    view.update(cx, |view, _| view.browser.set_class_of(removable));
    for key in keys {
        press(cx, window, &view, key);
    }
    settle(cx, &view);
    (window, view)
}

/// The switch row's checked state, or `None` when the dialog has no switch.
fn switch_state(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<bool> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("switch")
            .map(|found| found.checked() == Some(true))
    })
    .expect("window")
}

/// Says whether the pending plan carries a verify counter.
fn verifies(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> bool {
    view.read_with(cx, |view, _| {
        let id = view.browser.pending().expect("a pending plan").id;
        view.browser.verifies(id).is_some()
    })
}

#[gpui_kit::test]
fn a_copy_to_a_removable_drive_shows_the_switch_on(cx: &mut TestAppContext) {
    let (window, view) = dialog_after(cx, &["f5"]);
    assert!(verifies(cx, &view));
    assert_eq!(switch_state(cx, window), Some(true));
}

#[gpui_kit::test]
fn a_delete_shows_no_switch(cx: &mut TestAppContext) {
    let (window, view) = dialog_after(cx, &["space", "delete"]);
    assert!(view.read_with(cx, |view, _| view.browser.pending().is_some()));
    assert_eq!(switch_state(cx, window), None);
}

#[gpui_kit::test]
fn a_click_on_the_switch_turns_verify_off_and_on(cx: &mut TestAppContext) {
    let (window, view) = dialog_after(cx, &["f5"]);
    for on in [false, true] {
        cx.update_window(window, |_, window, cx| window.click("switch", cx))
            .expect("window");
        cx.run_until_parked();
        assert_eq!(verifies(cx, &view), on);
        assert_eq!(switch_state(cx, window), Some(on));
    }
}
