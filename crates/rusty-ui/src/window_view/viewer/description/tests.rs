use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px, size};

use crate::window_view::FerriteWindow;
use crate::window_view::tests::press;
use crate::window_view::viewer::ViewRich;
use crate::window_view::viewer::tests::structured::open_named;
use crate::window_view::viewer::tests::{Setup, label, redraw, text_of, visible};

const DBC: &[u8] = br#"VERSION ""
BU_: Alpha Beta
BO_ 42 First: 8 Alpha
 SG_ Speed : 0|8@1+ (1,0) [0|255] "rpm" Receiver
BO_ 43 Second: 8 Beta
 SG_ Torque : 0|8@1+ (1,0) [0|255] "Nm" Receiver
BO_ 44 Third: 8 Vector__XXX
 SG_ State : 0|8@1+ (1,0) [0|255] "" Receiver
"#;

const A2L: &[u8] = br#"ASAP2_VERSION 1 71
/begin PROJECT Invented ""
/begin MODULE Demo ""
/begin CHARACTERISTIC Limit "" VALUE 0x100 Layout 0 NO_COMPU_METHOD 0 255
/end CHARACTERISTIC
/begin MEASUREMENT Speed "" UWORD Scale 1 0 0 255
 ECU_ADDRESS 0x1234
/end MEASUREMENT
/begin COMPU_METHOD Scale "" LINEAR "%6.2" "rpm"
 COEFFS_LINEAR 1 0
/end COMPU_METHOD
/end MODULE
/end PROJECT
"#;

fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view, _) = open_named(cx, "sample.dbc", Setup::new(DBC.to_vec()));
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(ViewRich), cx);
    })
    .expect("window");
    redraw(cx, window);
    (window, view)
}

fn run_item(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
    text: &str,
) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
    cx.run_until_parked();
    press(cx, window, view, "enter");
    press(cx, window, view, "escape");
}

fn selected(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> bool {
    cx.update_window(window, |_, window, _| {
        window.find(("description-row", row)).selected() == Some(true)
    })
    .expect("window")
}

fn has_row(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> bool {
    cx.update_window(window, |_, window, _| {
        window.try_find(("description-row", row)).is_some()
    })
    .expect("window")
}

#[gpui_kit::test]
fn dbc_opens_with_three_sender_groups_unfolded(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    for (row, name) in [(0, "Alpha"), (3, "Beta"), (6, "No sender")] {
        assert!(label(cx, window, ("description-row", row)).starts_with(name));
        assert!(has_row(cx, window, row + 2));
    }
    assert!(selected(cx, window, 0));
    assert_eq!(
        text_of(cx, window, "description-inspector"),
        "Pick a message or a signal."
    );
}

#[gpui_kit::test]
fn left_selects_a_signals_message_then_folds_it(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    for key in ["down", "down"] {
        press(cx, window, &view, key);
    }
    assert!(selected(cx, window, 2));
    press(cx, window, &view, "left");
    assert!(selected(cx, window, 1));
    assert!(has_row(cx, window, 2));
    press(cx, window, &view, "left");
    assert!(!has_row(cx, window, 2));
    assert!(selected(cx, window, 1));
    press(cx, window, &view, "right");
    assert!(has_row(cx, window, 2));
}

#[gpui_kit::test]
fn enter_toggles_groups_and_movement_skips_folded_children(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    press(cx, window, &view, "enter");
    assert!(!has_row(cx, window, 1));
    press(cx, window, &view, "j");
    assert!(selected(cx, window, 3));
    press(cx, window, &view, "k");
    assert!(selected(cx, window, 0));
    press(cx, window, &view, "enter");
    assert!(has_row(cx, window, 2));
    press(cx, window, &view, "end");
    assert!(selected(cx, window, 8));
    press(cx, window, &view, "home");
    assert!(selected(cx, window, 0));
}

#[gpui_kit::test]
fn drawer_toggles_keep_the_selection_and_folds(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    for key in ["down", "left", "ctrl-b"] {
        press(cx, window, &view, key);
    }
    assert!(!visible(cx, window, "description-outline"));
    press(cx, window, &view, "ctrl-b");
    assert!(visible(cx, window, "description-outline"));
    assert!(selected(cx, window, 1));
    assert!(!has_row(cx, window, 2));
    let shown = visible(cx, window, "description-inspector");
    press(cx, window, &view, "ctrl-i");
    assert_ne!(visible(cx, window, "description-inspector"), shown);
    press(cx, window, &view, "ctrl-i");
    assert_eq!(visible(cx, window, "description-inspector"), shown);
}

#[gpui_kit::test]
fn presets_and_resize_keep_the_selected_item_and_fold_state(cx: &mut TestAppContext) {
    let (window, view) = open(cx);
    for key in ["down", "left", "alt-v"] {
        press(cx, window, &view, key);
    }
    run_item(cx, window, &view, "Simple");
    assert!(!visible(cx, window, "description-inspector"));
    press(cx, window, &view, "alt-v");
    run_item(cx, window, &view, "Rich");
    assert!(visible(cx, window, "description-inspector"));
    press(cx, window, &view, "alt-v");
    run_item(cx, window, &view, "Custom");
    assert!(visible(cx, window, "description-inspector"));
    cx.simulate_window_resize(window, size(px(1000.), px(700.)));
    redraw(cx, window);
    assert!(selected(cx, window, 1));
    assert!(!has_row(cx, window, 2));
}

#[gpui_kit::test]
fn clicking_a_group_folds_it_and_clicking_an_item_selects_it(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    cx.update_window(window, |_, window, cx| {
        window.click(("description-row", 1usize), cx);
    })
    .expect("window");
    redraw(cx, window);
    assert!(selected(cx, window, 1));
    assert!(!has_row(cx, window, 2));
    cx.update_window(window, |_, window, cx| {
        window.click(("description-row", 5usize), cx);
    })
    .expect("window");
    redraw(cx, window);
    assert!(selected(cx, window, 5));
}

#[gpui_kit::test]
fn a2l_opens_with_its_module_info_and_groups(cx: &mut TestAppContext) {
    let (window, view, _) = open_named(cx, "sample.a2l", Setup::new(A2L.to_vec()));
    press(cx, window, &view, "ctrl-i");
    for (row, name) in [
        (0, "Module Demo"),
        (1, "Characteristics (1)"),
        (2, "VALUE"),
        (3, "Limit"),
        (4, "Measurements (1)"),
        (5, "Speed"),
        (6, "Compu methods (1)"),
        (7, "Scale"),
    ] {
        assert!(label(cx, window, ("description-row", row)).starts_with(name));
    }
    assert!(selected(cx, window, 1));
    press(cx, window, &view, "up");
    assert!(selected(cx, window, 1));
    press(cx, window, &view, "end");
    assert!(selected(cx, window, 7));
    press(cx, window, &view, "home");
    assert!(selected(cx, window, 1));
    cx.update_window(window, |_, window, cx| {
        window.click(("description-row", 0usize), cx);
    })
    .expect("window");
    redraw(cx, window);
    assert!(selected(cx, window, 1));
    assert!(has_row(cx, window, 3));
    assert_eq!(
        text_of(cx, window, "description-inspector"),
        "Pick a characteristic or a measurement."
    );
}

#[gpui_kit::test]
fn with_reduced_motion_the_drawers_still_toggle_and_keep_the_selection(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (window, view) = open(cx);
    for key in ["down", "ctrl-b"] {
        press(cx, window, &view, key);
    }
    assert!(!visible(cx, window, "description-outline"));
    for key in ["ctrl-b", "ctrl-i", "ctrl-i"] {
        press(cx, window, &view, key);
    }
    assert!(visible(cx, window, "description-outline"));
    assert!(selected(cx, window, 1));
}
