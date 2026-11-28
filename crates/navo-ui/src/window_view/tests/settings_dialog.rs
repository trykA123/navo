//! Settings as its own floating window: no overlay scrim in Navo, resize, click in Navo,
//! close button and focus return.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Bounds, Modifiers, Pixels, Size, TestAppContext, VisualTestContext, point, px,
    size,
};

use super::app_menu::{Rig, Target, rig};
use crate::tokens::ThemeChoice;

fn open_in(cx: &mut TestAppContext, rig: &Rig, window: Size<Pixels>) {
    rig.press(cx, "ctrl-,");
    settle(cx, rig);
    let settings = rig.settings_window(cx).expect("settings window");
    VisualTestContext::from_window(settings, cx).simulate_resize(window);
    settle(cx, rig);
}

fn settle(cx: &mut TestAppContext, rig: &Rig) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    if let Some(settings) = rig.settings_window(cx) {
        let _ = cx.update_window(settings, |_, window, cx| window.render_frame(cx));
    }
}

fn bounds_in(
    cx: &mut TestAppContext,
    rig: &Rig,
    target: Target,
    id: &'static str,
) -> Option<Bounds<Pixels>> {
    let window = match target {
        Target::Navo => rig.window,
        Target::Settings => rig.settings_window(cx)?,
    };
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .filter(|found| found.visible())
            .map(|found| found.bounds())
    })
    .expect("window")
}

fn settings_viewport(cx: &mut TestAppContext, rig: &Rig) -> Size<Pixels> {
    let settings = rig.settings_window(cx).expect("settings window");
    cx.update_window(settings, |_, window, _| window.viewport_size())
        .expect("settings window")
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view.read_with(cx, |view, _| view.settings.is_open())
}

#[gpui_kit::test]
fn settings_open_in_their_own_window_without_a_scrim_in_navo(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_in(cx, &rig, size(px(1040.), px(800.)));
    assert!(is_open(cx, &rig));
    assert!(rig.settings_window(cx).is_some());
    assert!(bounds_in(cx, &rig, Target::Settings, "settings-window").is_some());
    assert!(bounds_in(cx, &rig, Target::Navo, "settings-sheet").is_none());
    assert!(bounds_in(cx, &rig, Target::Navo, "side-sheet-scrim").is_none());
}

#[gpui_kit::test]
fn settings_fit_inside_a_small_window_and_keep_the_close_button_visible(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_in(cx, &rig, size(px(800.), px(560.)));
    let viewport = settings_viewport(cx, &rig);
    let root = bounds_in(cx, &rig, Target::Settings, "settings-window").expect("settings window");
    assert_eq!(root.size, viewport);
    let bar = bounds_in(cx, &rig, Target::Settings, "title-bar").expect("the title bar shows");
    assert!(root.contains(&bar.center()), "bar {bar:?} in {root:?}");
    if let Some(close) = bounds_in(cx, &rig, Target::Settings, "close") {
        assert!(
            root.contains(&close.center()),
            "close {close:?} in {root:?}"
        );
    }
}

#[gpui_kit::test]
fn a_click_in_navo_leaves_settings_open_and_navo_active(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    let left = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    rig.press(cx, "tab");
    open_in(cx, &rig, size(px(1040.), px(800.)));
    let settings = rig.settings_window(cx).expect("settings window");
    let row = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let row = window
                .within(("list", left.get()))
                .find(("row", 0_usize))
                .bounds();
            point(row.left() + px(4.), row.center().y)
        })
        .expect("window");
    cx.update_window(rig.window, |_, window, _| window.activate_window())
        .expect("navo window");
    VisualTestContext::from_window(rig.window, cx).simulate_click(row, Modifiers::none());
    settle(cx, &rig);
    assert!(is_open(cx, &rig), "a click in Navo leaves Settings open");
    assert_eq!(rig.settings_window(cx), Some(settings));
    let navo_active = cx
        .update_window(rig.window, |_, window, _| window.is_window_active())
        .expect("navo window");
    assert!(navo_active, "Navo is active after a click inside it");
    let focused = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_eq!(focused, left, "the click lands directly on the Navo pane");
}

#[gpui_kit::test]
fn the_close_button_closes_settings_and_returns_focus_to_the_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_in(cx, &rig, size(px(1040.), px(800.)));
    if bounds_in(cx, &rig, Target::Settings, "close").is_some() {
        rig.click_in(cx, Target::Settings, "close");
    } else {
        rig.press_in(cx, Target::Settings, "ctrl-,");
    }
    assert!(!is_open(cx, &rig));
    assert_eq!(rig.settings_window(cx), None);
    assert!(rig.pane_has_focus(cx));
    settle(cx, &rig);
    assert!(bounds_in(cx, &rig, Target::Navo, "side-sheet-scrim").is_none());
}

#[gpui_kit::test]
fn a_click_after_closing_settings_reaches_the_pane_without_a_scrim(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    let left = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    rig.press(cx, "tab");
    open_in(cx, &rig, size(px(1040.), px(800.)));
    rig.press_in(cx, Target::Settings, "escape");
    let row = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("side-sheet-scrim").is_none());
            let row = window
                .within(("list", left.get()))
                .find(("row", 0_usize))
                .bounds();
            point(row.left() + px(4.), row.center().y)
        })
        .expect("window");
    VisualTestContext::from_window(rig.window, cx).simulate_click(row, Modifiers::none());
    let focused = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_eq!(focused, left, "the click lands directly on the pane");
}
