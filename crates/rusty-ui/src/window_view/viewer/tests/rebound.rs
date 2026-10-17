use std::time::Duration;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext};

use super::find::numbered_lines;
use super::structured::fake_with;
use super::{Log, Setup, hooks, options_with, paged, redraw, visible};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::shortcuts::{Rig, rig_on};
use crate::window_view::tests::{fake, settle, viewer_open};
use crate::window_view::viewer::Kinds;

const STORE: &[u8] = br#"{"store": {"book": [{"title": "Sayings"}]}, "name": "shop"}"#;

fn rig_for(cx: &mut TestAppContext, fs: std::sync::Arc<rusty_fs::FakeFs>, setup: Setup) -> Rig {
    fs.add_file("dest/keymap.toml", 40);
    let file = fs.location("dest/keymap.toml");
    let files = hooks(setup, &Log::default());
    rig_on(cx, options_with(fs, files, Kinds::builtin()), file)
}

fn record_back(rig: &Rig, cx: &mut TestAppContext, key: &str) {
    rig.open_page(cx);
    rig.record_on(cx, "viewer.back", key);
    assert!(!rig.is_recording(cx));
    rig.press(cx, "escape");
}

fn open_file(rig: &Rig, cx: &mut TestAppContext) {
    rig.press(cx, "f3");
    redraw(cx, rig.window);
    assert!(viewer_open(cx, &rig.view));
}

fn filter_tree(rig: &Rig, cx: &mut TestAppContext) {
    for key in ["t", "i"] {
        rig.press(cx, key);
    }
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &rig.view);
    redraw(cx, rig.window);
    assert!(visible(cx, rig.window, "filter"));
}

#[gpui_kit::test]
fn an_unbound_back_leaves_the_tree_and_a_recorded_key_works_there(cx: &mut TestAppContext) {
    let fs = fake_with("shop.json", STORE.len() as u64);
    let rig = rig_for(cx, fs, Setup::new(STORE.to_vec()));
    record_back(&rig, cx, "backspace");
    rig.press(cx, "down");
    open_file(&rig, cx);
    filter_tree(&rig, cx);
    rig.press(cx, "escape");
    assert!(
        !viewer_open(cx, &rig.view),
        "Escape no longer clears the filter"
    );
    record_back(&rig, cx, "f8");
    open_file(&rig, cx);
    filter_tree(&rig, cx);
    rig.press(cx, "f8");
    assert!(!visible(cx, rig.window, "filter"));
    assert!(viewer_open(cx, &rig.view));
}

fn focus_paged_body(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) {
    let body = paged(cx, view);
    cx.update_window(window, |_, window, cx| {
        let focus = body.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    })
    .expect("window");
    settle(cx, view);
}

fn open_find_bar(rig: &Rig, cx: &mut TestAppContext) {
    open_file(rig, cx);
    rig.press(cx, "ctrl-f");
    assert!(visible(cx, rig.window, "paged-bar"));
    focus_paged_body(cx, rig.window, &rig.view);
}

#[gpui_kit::test]
fn an_unbound_back_leaves_paged_text_and_a_recorded_key_works_there(cx: &mut TestAppContext) {
    let rig = rig_for(cx, fake(), Setup::huge(numbered_lines()));
    record_back(&rig, cx, "backspace");
    rig.press(cx, "down");
    open_find_bar(&rig, cx);
    rig.press(cx, "escape");
    assert!(
        !viewer_open(cx, &rig.view),
        "Escape no longer closes the find bar"
    );
    record_back(&rig, cx, "f8");
    open_find_bar(&rig, cx);
    rig.press(cx, "f8");
    assert!(!visible(cx, rig.window, "paged-bar"));
    assert!(viewer_open(cx, &rig.view));
}
