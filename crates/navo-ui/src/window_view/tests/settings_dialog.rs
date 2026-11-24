//! Settings as a floating dialog: size and margin at large and small windows, scrim click,
//! close button and focus return.

use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Bounds, Modifiers, Pixels, Size, TestAppContext, VisualTestContext, point, px,
    size,
};

use super::app_menu::{Rig, rig};
use crate::tokens::ThemeChoice;

const MARGIN: f32 = 24.;

fn open_in(cx: &mut TestAppContext, rig: &Rig, window: Size<Pixels>) {
    VisualTestContext::from_window(rig.window, cx).simulate_resize(window);
    rig.press(cx, "ctrl-,");
    settle(cx, rig);
}

fn settle(cx: &mut TestAppContext, rig: &Rig) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

fn bounds_of(cx: &mut TestAppContext, rig: &Rig, id: &'static str) -> Option<Bounds<Pixels>> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .filter(|found| found.visible())
            .map(|found| found.bounds())
    })
    .expect("window")
}

fn viewport(cx: &mut TestAppContext, rig: &Rig) -> Size<Pixels> {
    cx.update_window(rig.window, |_, window, _| window.viewport_size())
        .expect("window")
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.view.read_with(cx, |view, _| view.settings.is_open())
}

fn assert_inside_with_margin(dialog: Bounds<Pixels>, window: Size<Pixels>) {
    let margin = px(MARGIN);
    assert!(dialog.left() >= margin, "left edge {dialog:?}");
    assert!(dialog.top() >= margin, "top edge {dialog:?}");
    assert!(
        dialog.right() <= window.width - margin,
        "right edge {dialog:?}"
    );
    assert!(
        dialog.bottom() <= window.height - margin,
        "bottom edge {dialog:?}"
    );
}

#[gpui_kit::test]
fn settings_float_as_a_centred_dialog_of_bounded_size_in_a_large_window(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_in(cx, &rig, size(px(1900.), px(1950.)));
    let window = viewport(cx, &rig);
    let dialog = bounds_of(cx, &rig, "settings-sheet").expect("the settings dialog shows");
    assert_eq!(dialog.size, size(px(1040.), px(800.)), "{dialog:?}");
    assert!(
        (dialog.center().x - window.width / 2.).abs() <= px(1.),
        "{dialog:?}"
    );
    assert_inside_with_margin(dialog, window);
    let scrim = bounds_of(cx, &rig, "side-sheet-scrim").expect("a scrim dims the window");
    assert!(scrim.size.width >= window.width - px(1.), "{scrim:?}");
}

#[gpui_kit::test]
fn settings_shrink_with_a_small_window_and_keep_the_footer_visible(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_in(cx, &rig, size(px(800.), px(560.)));
    let window = viewport(cx, &rig);
    let dialog = bounds_of(cx, &rig, "settings-sheet").expect("the settings dialog shows");
    assert_inside_with_margin(dialog, window);
    let footer = bounds_of(cx, &rig, "side-sheet-close").expect("the close button shows");
    assert!(
        dialog.contains(&footer.center()),
        "close {footer:?} in {dialog:?}"
    );
}

#[gpui_kit::test]
fn a_click_on_the_scrim_closes_settings_and_returns_focus_to_the_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "tab");
    let before = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    open_in(cx, &rig, size(px(1600.), px(1000.)));
    cx.update_window(rig.window, |_, window, cx| {
        window.click_at("settings-sheet", point(px(400.), px(20.)), cx);
    })
    .expect("window");
    settle(cx, &rig);
    assert!(is_open(cx, &rig), "a click inside the dialog keeps it open");
    cx.update_window(rig.window, |_, window, cx| {
        window.click_at("side-sheet-scrim", point(px(8.), px(8.)), cx);
    })
    .expect("window");
    settle(cx, &rig);
    assert!(!is_open(cx, &rig));
    assert!(rig.pane_has_focus(cx));
    let after = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_eq!(after, before, "focus returns to the pane that had it");
}

#[gpui_kit::test]
fn the_close_button_closes_settings_and_returns_focus_to_the_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    open_in(cx, &rig, size(px(1600.), px(1000.)));
    rig.click(cx, "side-sheet-close");
    assert!(!is_open(cx, &rig));
    assert!(rig.pane_has_focus(cx));
    settle(cx, &rig);
    assert!(bounds_of(cx, &rig, "side-sheet-scrim").is_none());
}

#[gpui_kit::test]
fn a_click_during_the_fade_out_reaches_the_pane_under_the_scrim(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    let left = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    rig.press(cx, "tab");
    open_in(cx, &rig, size(px(1600.), px(1000.)));
    rig.press(cx, "escape");
    let row = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let scrim = window
                .try_find("side-sheet-scrim")
                .map(|found| found.bounds());
            let row = window
                .within(("list", left.get()))
                .find(("row", 0_usize))
                .bounds();
            (scrim, point(row.left() + px(4.), row.center().y))
        })
        .expect("window");
    assert!(row.0.is_some(), "the scrim is still fading out");
    VisualTestContext::from_window(rig.window, cx).simulate_click(row.1, Modifiers::none());
    let focused = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_eq!(
        focused, left,
        "the click lands on the pane, not on the leaving scrim"
    );
}
