use gpui_kit::{TestAppContext, px};

use super::{frame, open, open_closed, reduce_motion};
use crate::side_sheet::Side;
use crate::side_sheet::motion::{entering, leaving};

#[test]
fn reduced_motion_keeps_the_fade() {
    for look in [entering(0., true), entering(0.5, true), leaving(0.5, true)] {
        assert_eq!(look.outward, px(0.));
    }
    assert_eq!(entering(0., true).opacity, 0.);
    let fading_in = entering(0.5, true).opacity;
    let fading_out = leaving(0.5, true).opacity;
    assert!(fading_in > 0. && fading_in < 1., "{fading_in}");
    assert!(fading_out > 0. && fading_out < 1., "{fading_out}");
    assert_eq!(entering(1., true).opacity, 1.);
}

#[gpui_kit::test]
fn a_right_sheet_slides_28_px_in_from_the_window_edge(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    assert_eq!(rig.sheet(cx).right(), px(1424. + 28.));
    rig.wait(100, cx);
    let halfway = rig.sheet(cx).right();
    assert!(halfway > px(1424.) && halfway < px(1452.), "{halfway:?}");
    rig.settle(cx);
    assert_eq!(rig.sheet(cx).right(), px(1424.));
}

#[gpui_kit::test]
fn a_left_sheet_slides_in_from_the_left(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Right));
    assert_eq!(rig.sheet(cx).left(), px(-28.));
    rig.settle(cx);
    assert_eq!(rig.sheet(cx).left(), px(0.));
}

#[gpui_kit::test]
fn a_new_active_pane_replays_the_enter_on_the_other_side(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.set_frame(frame(1440., Side::Right), cx);
    assert_eq!(rig.sheet(cx).left(), px(-28.));
    rig.settle(cx);
    assert_eq!(rig.sheet(cx).left(), px(0.));
}

#[gpui_kit::test]
fn the_sheet_leaves_faster_than_it_enters_and_then_is_gone(cx: &mut TestAppContext) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    rig.hide(cx);
    let state = rig.state(cx);
    assert!(rig.bounds(cx).is_some());
    assert!(cx.update(|cx| state.read(cx).is_shown(cx)));
    rig.wait(90, cx);
    let leaving = rig.sheet(cx).right();
    assert!(leaving > px(1424.), "{leaving:?}");
    rig.wait(120, cx);
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
    rig.wait(90, cx);
    assert_eq!(rig.sheet(cx).left(), px(0.));
    rig.wait(120, cx);
    assert!(rig.bounds(cx).is_none());
}

#[gpui_kit::test]
fn a_closed_sheet_renders_nothing(cx: &mut TestAppContext) {
    let rig = open_closed(cx, frame(1440., Side::Left));
    rig.frame(cx);
    assert!(rig.bounds(cx).is_none());
}
