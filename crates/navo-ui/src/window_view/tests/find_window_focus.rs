use std::sync::mpsc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Role, TestAppContext, VisualTestContext};
use navo_core::Location;

use super::super::NavoWindow;
use super::{fake, open_options, options_for, press, settle, two_panes};
use crate::find_results::run::{FindRun, RunEvent};

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    settle(cx, &view);
    Rig { window, view }
}

impl Rig {
    fn add_tab(&self, cx: &mut TestAppContext, text: &str) -> u64 {
        self.view.update(cx, |view, cx| {
            view.find.book().update(cx, |book, cx| {
                let id = book.add(text, Location::new("/tmp/find").expect("test location"));
                cx.notify();
                id
            })
        })
    }

    fn find_window(&self, cx: &TestAppContext) -> Option<AnyWindowHandle> {
        self.view.read_with(cx, |view, _| view.find.window_handle())
    }

    fn is_active(&self, cx: &mut TestAppContext, handle: AnyWindowHandle) -> bool {
        cx.update_window(handle, |_, window, _| window.is_window_active())
            .expect("window")
    }

    fn core_pane_focused(&self, cx: &mut TestAppContext) -> bool {
        let pane = self
            .view
            .read_with(cx, |view, _| view.browser.state().focused_pane());
        let focus = self.view.update(cx, |view, cx| view.handle(pane, cx));
        cx.update_window(self.window, |_, window, _| focus.is_focused(window))
            .expect("navo window")
    }

    fn press_in(&self, cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn ctrl_shift_j_with_no_tabs_opens_command_center_in_content_mode(cx: &mut TestAppContext) {
    let rig = rig(cx);
    assert!(
        rig.view
            .read_with(cx, |view, cx| view.find.book().read(cx).is_empty())
    );

    press(cx, rig.window, &rig.view, "ctrl-shift-j");

    assert_eq!(rig.find_window(cx), None);
    assert!(
        rig.view
            .read_with(cx, |view, _| view.command_center.is_open())
    );
    let content_index = crate::command_center::Mode::ALL
        .iter()
        .position(|mode| *mode == crate::command_center::Mode::Content)
        .expect("Content mode");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let input = window.find("command-center-input");
        assert!(input.visible());
        assert_eq!(
            window
                .find(("command-center-mode", content_index))
                .checked(),
            Some(true)
        );
    })
    .expect("navo window");
}

#[gpui_kit::test]
fn ctrl_shift_j_and_idle_escape_switch_focus_between_navo_and_find_window(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let _ = rig.add_tab(cx, "needle");

    press(cx, rig.window, &rig.view, "ctrl-shift-j");
    let find = rig
        .find_window(cx)
        .expect("ctrl-shift-j with tabs should open find window");
    assert!(rig.is_active(cx, find));

    // Ctrl+Shift+J inside FindWindow returns focus to Navo's core pane.
    rig.press_in(cx, find, "ctrl-shift-j");
    assert!(rig.is_active(cx, rig.window));
    assert!(rig.core_pane_focused(cx));

    // Hide FindWindow, then Ctrl+Shift+J in Navo shows and focuses it again.
    let navo = rig.window;
    rig.view.update(cx, |view, cx| view.hide_find(navo, cx));
    cx.run_until_parked();
    VisualTestContext::from_window(find, cx).deactivate_window();
    cx.run_until_parked();

    press(cx, rig.window, &rig.view, "ctrl-shift-j");
    assert_eq!(rig.find_window(cx), Some(find));
    assert!(!rig.view.read_with(cx, |view, _| view.find.is_hidden()));
    assert!(rig.is_active(cx, find));

    // Esc while idle returns focus to Navo's core pane.
    rig.press_in(cx, find, "escape");
    assert!(rig.is_active(cx, rig.window));
    assert!(rig.core_pane_focused(cx));
}

#[gpui_kit::test]
fn escape_stops_running_search_without_leaving_and_second_escape_returns_to_navo(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx);
    let id = rig.add_tab(cx, "needle");
    let (_send, receive) = mpsc::sync_channel::<RunEvent>(4);
    rig.view.update(cx, |view, cx| {
        view.find.book().update(cx, |book, cx| {
            book.tab_mut(id).expect("tab").run = Some(FindRun::from_receiver(receive));
            cx.notify();
        });
    });

    press(cx, rig.window, &rig.view, "ctrl-shift-j");
    let find = rig.find_window(cx).expect("find window");
    assert!(rig.is_active(cx, find));
    assert!(rig.view.read_with(cx, |view, cx| {
        view.find
            .book()
            .read(cx)
            .active()
            .is_some_and(|tab| tab.is_running())
    }));

    // First Esc stops the active run and keeps focus in FindWindow.
    rig.press_in(cx, find, "escape");
    assert!(!rig.view.read_with(cx, |view, cx| {
        view.find
            .book()
            .read(cx)
            .active()
            .is_some_and(|tab| tab.is_running())
    }));
    assert!(rig.is_active(cx, find));

    // Second Esc (now idle) activates Navo and focuses the core pane.
    rig.press_in(cx, find, "escape");
    assert!(rig.is_active(cx, rig.window));
    assert!(rig.core_pane_focused(cx));
}

#[gpui_kit::test]
fn clicking_status_bar_find_chip_opens_and_focuses_find_window(cx: &mut TestAppContext) {
    let rig = rig(cx);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("status-find-chip").is_none());
    })
    .expect("navo window");

    let _ = rig.add_tab(cx, "needle");

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let chip = window.find("status-find-chip");
        assert_eq!(chip.role(), Some(Role::Button));
        window.click("status-find-chip", cx);
    })
    .expect("navo window");
    cx.run_until_parked();

    let find = rig
        .find_window(cx)
        .expect("clicking status-find-chip should open find window");
    assert!(rig.is_active(cx, find));
}
