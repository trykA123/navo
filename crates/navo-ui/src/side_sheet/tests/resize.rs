use gpui_kit::{TestAppContext, px};

use super::{frame, open, open_closed};
use crate::side_sheet::Side;

#[gpui_kit::test]
fn dragging_the_grip_inward_widens_a_right_sheet(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.drag_grip_by(-100., cx);
    let sheet = rig.sheet(cx);
    assert_eq!(sheet.size.width, px(698.));
    assert_eq!(sheet.right(), px(1424.));
    let state = rig.state(cx);
    assert_eq!(state.read_with(cx, |state, _| state.delta()), px(100.));
}

#[gpui_kit::test]
fn dragging_the_grip_inward_widens_a_left_sheet(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Right));
    rig.settle(cx);
    rig.drag_grip_by(100., cx);
    let sheet = rig.sheet(cx);
    assert_eq!(sheet.size.width, px(698.));
    assert_eq!(sheet.left(), px(0.));
}

#[gpui_kit::test]
fn dragging_past_the_limits_stops_at_the_clamps(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.drag_grip_by(500., cx);
    assert_eq!(rig.sheet(cx).size.width, px(320.));
    rig.drag_grip_by(-1100., cx);
    assert_eq!(rig.sheet(cx).size.width, px(1064.));
    rig.press("ctrl-[", cx);
    rig.frame(cx);
    assert_eq!(rig.sheet(cx).size.width, px(1016.));
}

#[gpui_kit::test]
fn ctrl_brackets_resize_in_48_px_steps(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.press("ctrl-]", cx);
    rig.frame(cx);
    assert_eq!(rig.sheet(cx).size.width, px(646.));
    rig.press("ctrl-[", cx);
    rig.press("ctrl-[", cx);
    rig.frame(cx);
    assert_eq!(rig.sheet(cx).size.width, px(550.));
}

#[gpui_kit::test]
fn ctrl_brackets_do_nothing_while_the_sheet_is_closed(cx: &mut TestAppContext) {
    let rig = open_closed(cx, frame(1440., Side::Left));
    rig.press("ctrl-]", cx);
    let state = rig.state(cx);
    assert_eq!(state.read_with(cx, |state, _| state.delta()), px(0.));
}
