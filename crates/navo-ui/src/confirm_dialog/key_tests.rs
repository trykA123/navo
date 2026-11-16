// Confirm-dialog tests that press keys. They need the host's key bindings (Navo's
// `test_window`), so they stay in Navo; the kit move must not take this file.
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Role, TestAppContext};

use super::ConfirmProps;
use super::tests::{
    Counts, DIALOG, Harness, counts, focused, press, props, toggled, type_text, typed, with_choice,
};
use crate::test_window::open;

fn open_dialog(cx: &mut TestAppContext, props: ConfirmProps) -> (AnyWindowHandle, Entity<Harness>) {
    let (window, harness) = open(cx, |_, _| Harness {
        props,
        counts: Counts::default(),
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
    (window, harness)
}

#[gpui_kit::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(window, &["escape"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn with_danger_enter_right_away_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.within(DIALOG).find("cancel").focused(), Some(true));
        assert_eq!(window.within(DIALOG).find("confirm").focused(), Some(false));
    })
    .expect("window");
    press(window, &["enter"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn with_danger_tab_then_enter_confirms(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(window, &["tab", "enter"], cx);
    assert_eq!(counts(&harness, cx), (1, 0));
}

#[gpui_kit::test]
fn tab_and_shift_tab_move_between_the_buttons(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(
        window,
        &["tab", "tab", "shift-tab", "shift-tab", "enter"],
        cx,
    );
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn without_danger_enter_confirms(cx: &mut TestAppContext) {
    let mut safe = props(1, 0);
    safe.danger = false;
    let (window, harness) = open_dialog(cx, safe);
    press(window, &["enter"], cx);
    assert_eq!(counts(&harness, cx), (1, 0));
}

#[gpui_kit::test]
fn escape_in_the_field_cancels(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, typed());
    type_text(window, "DEL", cx);
    press(window, &["escape"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
}

#[gpui_kit::test]
fn tab_wraps_forward_and_shift_tab_wraps_back(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(1, 0));
    press(window, &["tab"], cx);
    assert_eq!(focused(window, cx), ["confirm"]);
    press(window, &["tab"], cx);
    assert_eq!(focused(window, cx), ["cancel"]);
    press(window, &["shift-tab"], cx);
    assert_eq!(focused(window, cx), ["confirm"]);
    press(window, &["shift-tab"], cx);
    assert_eq!(focused(window, cx), ["cancel"]);
}

#[gpui_kit::test]
fn the_arrow_keys_move_between_the_buttons_and_wrap(cx: &mut TestAppContext) {
    let (window, _) = open_dialog(cx, props(1, 0));
    press(window, &["left"], cx);
    assert_eq!(focused(window, cx), ["confirm"]);
    press(window, &["left"], cx);
    assert_eq!(focused(window, cx), ["cancel"]);
    press(window, &["right"], cx);
    assert_eq!(focused(window, cx), ["confirm"]);
    press(window, &["right"], cx);
    assert_eq!(focused(window, cx), ["cancel"]);
}

#[gpui_kit::test]
fn space_activates_the_focused_button(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, props(1, 0));
    press(window, &["space"], cx);
    assert_eq!(counts(&harness, cx), (0, 1));
    press(window, &["tab", "space"], cx);
    assert_eq!(counts(&harness, cx), (1, 1));
}

#[gpui_kit::test]
fn a_choice_is_a_checkbox_stop_before_the_buttons(cx: &mut TestAppContext) {
    let (window, harness) = open_dialog(cx, with_choice());
    assert_eq!(focused(window, cx), ["confirm"]);
    cx.update_window(window, |_, window, _| {
        assert_eq!(
            window.within(DIALOG).find("choice").role(),
            Some(Role::CheckBox)
        );
    })
    .expect("window");
    press(window, &["shift-tab"], cx);
    assert_eq!(focused(window, cx), ["choice"]);
    press(window, &["space"], cx);
    assert_eq!(toggled(&harness, cx), 1);
    assert_eq!(focused(window, cx), ["choice"]);
    assert_eq!(counts(&harness, cx), (0, 0));
    press(window, &["tab", "enter"], cx);
    assert_eq!(counts(&harness, cx), (1, 0));
}
