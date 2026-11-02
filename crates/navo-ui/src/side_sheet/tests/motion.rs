use gpui_kit::{TestAppContext, px};

use super::{frame, open, open_closed, reduce_motion};
use crate::side_sheet::Side;
use crate::side_sheet::motion::{entering, leaving};

#[test]
fn the_sheet_only_fades() {
    assert_eq!(entering(0.).opacity, 0.);
    let fading_in = entering(0.5).opacity;
    let fading_out = leaving(0.5).opacity;
    assert!(fading_in > 0. && fading_in < 1., "{fading_in}");
    assert!(fading_out > 0. && fading_out < 1., "{fading_out}");
    assert_eq!(entering(1.).opacity, 1.);
    assert!(entering(1.).is_settled);
}

#[gpui_kit::test]
fn a_right_sheet_opens_in_place(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    assert_eq!(rig.sheet(cx).right(), px(1424.));
    rig.wait(40, cx);
    assert_eq!(rig.sheet(cx).right(), px(1424.));
}

#[gpui_kit::test]
fn a_left_sheet_opens_in_place(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Right));
    assert_eq!(rig.sheet(cx).left(), px(0.));
}

#[gpui_kit::test]
fn a_new_active_pane_shows_the_sheet_on_the_other_side_at_once(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.set_frame(frame(1440., Side::Right), cx);
    assert_eq!(rig.sheet(cx).left(), px(0.));
}

#[gpui_kit::test]
fn the_sheet_fades_out_in_place_and_then_is_gone(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.hide(cx);
    let state = rig.state(cx);
    assert!(rig.bounds(cx).is_some());
    assert!(cx.update(|cx| state.read(cx).is_shown(cx)));
    rig.wait(40, cx);
    assert_eq!(rig.sheet(cx).right(), px(1424.));
    rig.wait(60, cx);
    assert!(rig.bounds(cx).is_none());
    assert!(!cx.update(|cx| state.read(cx).is_shown(cx)));
}

#[gpui_kit::test]
fn reduced_motion_fades_in_without_movement(cx: &mut TestAppContext) {
    reduce_motion(cx);
    let rig = open(cx, frame(1440., Side::Left));
    assert_eq!(rig.sheet(cx).right(), px(1424.));
    rig.wait(100, cx);
    assert_eq!(rig.sheet(cx).right(), px(1424.));
}

#[gpui_kit::test]
fn reduced_motion_fades_out_without_movement(cx: &mut TestAppContext) {
    reduce_motion(cx);
    let rig = open(cx, frame(1440., Side::Right));
    rig.settle(cx);
    rig.hide(cx);
    rig.wait(40, cx);
    assert_eq!(rig.sheet(cx).left(), px(0.));
    rig.wait(60, cx);
    assert!(rig.bounds(cx).is_none());
}

#[gpui_kit::test]
fn a_closed_sheet_renders_nothing(cx: &mut TestAppContext) {
    let rig = open_closed(cx, frame(1440., Side::Left));
    rig.frame(cx);
    assert!(rig.bounds(cx).is_none());
}
