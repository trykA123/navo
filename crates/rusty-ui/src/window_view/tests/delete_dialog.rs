use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, InputEvent as _, TestAppContext};
use rusty_fs::{FakeFs, Provider};

use super::{fake, home_beside_dest, open_with, press, settle};
use crate::window_view::FerriteWindow;

struct Rig {
    fs: Arc<FakeFs>,
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    fn is_open(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.browser.pending().is_some())
    }

    fn focused(&self, cx: &mut TestAppContext) -> Vec<&'static str> {
        self.window
            .update(cx, |_, window, _| {
                ["confirm", "cancel"]
                    .into_iter()
                    .filter(|id| {
                        window.try_find(*id).and_then(|found| found.focused()) == Some(true)
                    })
                    .collect()
            })
            .expect("window")
    }

    fn docs_exists(&self) -> bool {
        self.fs.metadata(&self.fs.location("home/docs")).is_ok()
    }

    fn jobs(&self, cx: &mut TestAppContext) -> usize {
        settle(cx, &self.view);
        self.view.read_with(cx, |view, _| view.browser.jobs().len())
    }
}

fn marked_folder(cx: &mut TestAppContext, delete: &str) -> Rig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    let rig = Rig { fs, window, view };
    rig.press(cx, &["space", delete]);
    assert!(rig.is_open(cx), "{delete} opens the confirm dialog");
    rig
}

fn on_cancel(cx: &mut TestAppContext, delete: &str) -> Rig {
    let rig = marked_folder(cx, delete);
    if rig.focused(cx) != ["cancel"] {
        rig.press(cx, &["tab"]);
    }
    assert_eq!(rig.focused(cx), ["cancel"]);
    rig
}

fn on_confirm(cx: &mut TestAppContext, delete: &str) -> Rig {
    let rig = marked_folder(cx, delete);
    if rig.focused(cx) != ["confirm"] {
        rig.press(cx, &["right"]);
    }
    assert_eq!(rig.focused(cx), ["confirm"]);
    rig
}

fn assert_cancelled(rig: &Rig, cx: &mut TestAppContext) {
    assert!(!rig.is_open(cx), "the dialog closes");
    assert!(rig.docs_exists(), "the marked folder is still there");
    assert_eq!(rig.jobs(cx), 0, "no job was started");
}

fn assert_confirmed(rig: &Rig, cx: &mut TestAppContext) {
    assert!(!rig.is_open(cx), "the dialog closes");
    assert_eq!(rig.jobs(cx), 1, "exactly one job was started");
    assert!(!rig.docs_exists(), "the marked folder is gone");
}

#[gpui_kit::test]
fn enter_on_cancel_of_the_recycle_dialog_closes_it(cx: &mut TestAppContext) {
    let rig = on_cancel(cx, "delete");
    rig.press(cx, &["enter"]);
    assert_cancelled(&rig, cx);
}

#[gpui_kit::test]
fn enter_on_cancel_of_the_delete_dialog_closes_it(cx: &mut TestAppContext) {
    let rig = on_cancel(cx, "shift-delete");
    rig.press(cx, &["enter"]);
    assert_cancelled(&rig, cx);
}

#[gpui_kit::test]
fn enter_on_recycle_moves_the_marked_folder_once(cx: &mut TestAppContext) {
    let rig = on_confirm(cx, "delete");
    rig.press(cx, &["enter"]);
    assert_confirmed(&rig, cx);
}

#[gpui_kit::test]
fn enter_on_delete_permanently_removes_the_marked_folder_once(cx: &mut TestAppContext) {
    let rig = on_confirm(cx, "shift-delete");
    rig.press(cx, &["enter"]);
    assert_confirmed(&rig, cx);
}

#[gpui_kit::test]
fn space_on_cancel_of_the_recycle_dialog_closes_it(cx: &mut TestAppContext) {
    let rig = on_cancel(cx, "delete");
    rig.press(cx, &["space"]);
    assert_cancelled(&rig, cx);
}

#[gpui_kit::test]
fn space_on_recycle_moves_the_marked_folder_once(cx: &mut TestAppContext) {
    let rig = on_confirm(cx, "delete");
    rig.press(cx, &["space"]);
    assert_confirmed(&rig, cx);
}

fn after_window_tab(rig: &Rig, cx: &mut TestAppContext) {
    rig.window
        .update(cx, |_, window, cx| window.focus_next(cx))
        .expect("window");
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn enter_after_the_window_tab_order_moved_focus_still_presses(cx: &mut TestAppContext) {
    let rig = on_cancel(cx, "shift-delete");
    after_window_tab(&rig, cx);
    after_window_tab(&rig, cx);
    rig.press(cx, &["enter"]);
    assert!(!rig.is_open(cx), "the dialog closes");
    assert!(
        rig.docs_exists() || rig.jobs(cx) == 1,
        "one button was pressed"
    );
}

fn typing_dialog(cx: &mut TestAppContext, button: &str) -> Rig {
    let fs = fake();
    fs.add_file("home/bab.txt", 1);
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    let rig = Rig { fs, window, view };
    rig.press(cx, &["space", "delete"]);
    if rig.focused(cx) != [button] {
        rig.press(cx, &["tab"]);
    }
    assert_eq!(rig.focused(cx), [button]);
    rig
}

fn key_down(rig: &Rig, cx: &mut TestAppContext, key: &str, is_held: bool) {
    rig.window
        .update(cx, |_, window, cx| {
            window.dispatch_event(
                gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse(key)
                        .expect("key")
                        .with_simulated_ime(),
                    is_held,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
        })
        .expect("window");
    settle(cx, &rig.view);
}

fn assert_no_filter(rig: &Rig, cx: &TestAppContext) {
    assert!(rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.filter_text(tab).is_none()
    }));
}

#[gpui_kit::test]
fn dialog_typing_does_not_filter_after_space_cancels(cx: &mut TestAppContext) {
    let rig = typing_dialog(cx, "cancel");
    rig.press(cx, &["a"]);
    key_down(&rig, cx, "b", false);
    rig.press(cx, &["space"]);
    key_down(&rig, cx, "space", true);
    key_down(&rig, cx, "b", true);
    assert_cancelled(&rig, cx);
    assert_no_filter(&rig, cx);
}

#[gpui_kit::test]
fn fresh_typing_filters_after_space_confirms_without_leaking_dialog_keys(cx: &mut TestAppContext) {
    let rig = typing_dialog(cx, "confirm");
    rig.press(cx, &["a"]);
    key_down(&rig, cx, "b", false);
    rig.press(cx, &["space"]);
    key_down(&rig, cx, "b", true);
    assert_confirmed(&rig, cx);
    assert_no_filter(&rig, cx);
    rig.press(cx, &["a"]);
    key_down(&rig, cx, "b", true);
    rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        assert_eq!(view.browser.filter_text(tab), Some("a"));
    });
    rig.press(cx, &["b"]);
    rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        assert_eq!(view.browser.filter_text(tab), Some("ab"));
    });
}
