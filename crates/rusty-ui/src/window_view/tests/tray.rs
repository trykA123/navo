use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext, VisualTestContext};
use rusty_core::Command;

use super::{close_in, fake, open_options, options_for, press, settle, two_panes, type_in};
use crate::settings::{Change, Field, Value};
use crate::window_view::{FerriteWindow, OpenRequest, OpenSender, WindowShown, open_requests};

type Calls = Arc<Mutex<Vec<bool>>>;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    sender: OpenSender,
    saves: Rc<Cell<usize>>,
    calls: Calls,
}

fn hook(calls: &Calls, hides: bool) -> WindowShown {
    let calls = calls.clone();
    Arc::new(move |_, shown| {
        calls.lock().unwrap().push(shown);
        hides
    })
}

fn rig_with(cx: &mut TestAppContext, keep_in_tray: bool, shown: Option<WindowShown>) -> Rig {
    let fs = fake();
    let saves = Rc::new(Cell::new(0));
    let count = saves.clone();
    let (sender, inbox) = open_requests();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.on_save = Rc::new(move |_| count.set(count.get() + 1));
    options.open.inbox = Some(inbox);
    options.open.window_shown = shown;
    options.settings.values = options.settings.values.with(&Change {
        field: Field::KeepInTray,
        value: Value::Flag(keep_in_tray),
    });
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        sender,
        saves,
        calls: Calls::default(),
    }
}

fn rig(cx: &mut TestAppContext, keep_in_tray: bool) -> Rig {
    let calls = Calls::default();
    let mut rig = rig_with(cx, keep_in_tray, Some(hook(&calls, true)));
    rig.calls = calls;
    rig
}

impl Rig {
    fn simulate_close(&self, cx: &mut TestAppContext) -> bool {
        VisualTestContext::from_window(self.window, cx).simulate_close()
    }

    fn calls(&self) -> Vec<bool> {
        self.calls.lock().unwrap().clone()
    }

    fn send(&self, cx: &mut TestAppContext, request: OpenRequest) {
        assert!(self.sender.send(request), "the window takes requests");
        settle(cx, &self.view);
    }

    fn is_active(&self, cx: &mut TestAppContext) -> bool {
        VisualTestContext::from_window(self.window, cx)
            .update(|window, _| window.is_window_active())
    }
}

fn quit_request() -> OpenRequest {
    OpenRequest {
        quit: true,
        ..OpenRequest::default()
    }
}

#[gpui_kit::test]
fn closing_with_keep_in_tray_on_saves_and_hides_without_quitting(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    assert!(!rig.simulate_close(cx));
    assert_eq!(rig.calls(), [false]);
    assert_eq!(rig.saves.get(), 1);
}

#[gpui_kit::test]
fn closing_with_keep_in_tray_off_saves_and_quits(cx: &mut TestAppContext) {
    let rig = rig(cx, false);
    assert!(rig.simulate_close(cx));
    assert!(rig.calls().is_empty());
    assert_eq!(rig.saves.get(), 1);
}

#[gpui_kit::test]
fn closing_without_a_hide_hook_quits_even_with_keep_in_tray_on(cx: &mut TestAppContext) {
    let rig = rig_with(cx, true, None);
    assert!(rig.simulate_close(cx));
    assert_eq!(rig.saves.get(), 1);
}

#[gpui_kit::test]
fn a_hook_that_cannot_hide_lets_the_window_close(cx: &mut TestAppContext) {
    let calls = Calls::default();
    let rig = rig_with(cx, true, Some(hook(&calls, false)));
    assert!(rig.simulate_close(cx));
    assert_eq!(*calls.lock().unwrap(), [false]);
}

#[gpui_kit::test]
fn a_hidden_window_keeps_autosaving(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    assert!(!rig.simulate_close(cx));
    rig.view.update(cx, |view, cx| {
        view.browser
            .command(&Command::WorkspaceNew { name: None })
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(rig.saves.get(), 2);
}

#[gpui_kit::test]
fn an_arrival_shows_the_hidden_window_and_activates_it(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    assert!(!rig.simulate_close(cx));
    VisualTestContext::from_window(rig.window, cx).deactivate_window();
    rig.send(cx, OpenRequest::default());
    assert_eq!(rig.calls(), [false, true]);
    assert!(rig.is_active(cx));
}

#[gpui_kit::test]
fn a_quit_arrival_saves_and_quits_without_showing(cx: &mut TestAppContext) {
    let rig = rig(cx, true);
    assert!(!rig.simulate_close(cx));
    rig.send(cx, quit_request());
    assert_eq!(rig.calls(), [false]);
    assert_eq!(rig.saves.get(), 2);
}

#[gpui_kit::test]
fn a_quit_arrival_with_unsaved_edits_shows_the_warning_instead(cx: &mut TestAppContext) {
    let calls = Calls::default();
    let (window, view, _) = super::viewer_window(cx, Ok("hello\n"), false);
    view.update(cx, |view, cx| {
        view.window_shown = Some(hook(&calls, true));
        cx.notify();
    });
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-e");
    type_in(cx, window, &view, "x");
    VisualTestContext::from_window(window, cx).deactivate_window();
    view.update_in(
        &mut VisualTestContext::from_window(window, cx),
        |view, window, cx| {
            view.quit_on_request(window, cx);
        },
    );
    assert_eq!(*calls.lock().unwrap(), [true]);
    assert!(super::viewer_open(cx, &view));
    assert!(close_in(cx, window, &view));
}
