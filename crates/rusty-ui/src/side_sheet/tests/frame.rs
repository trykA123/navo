use gpui_kit::{TestAppContext, px};

use super::{frame, open};
use crate::side_sheet::{SheetFrame, SheetState, Side, Step};

#[test]
fn the_sheet_sits_on_the_side_opposite_the_active_pane() {
    assert_eq!(frame(1440., Side::Left).side(), Side::Right);
    assert_eq!(frame(1440., Side::Right).side(), Side::Left);
}

#[test]
fn the_default_width_is_42_percent_of_the_body() {
    let wide = frame(1440., Side::Left);
    assert_eq!(wide.body, px(1424.));
    assert_eq!(SheetState::new().width(&wide), px(598.));
}

#[test]
fn the_sheet_is_never_narrower_than_320_px() {
    let small = SheetFrame::new(px(1200.), px(596.), Side::Left);
    assert_eq!(small.width(px(-1000.)), px(320.));
}

#[test]
fn the_active_card_always_keeps_360_px() {
    let wide = frame(1440., Side::Left);
    assert_eq!(wide.width(px(5000.)), px(1424. - 360.));
}

#[test]
fn the_320_px_floor_wins_over_the_active_card_in_a_tiny_body() {
    let tiny = SheetFrame::new(px(600.), px(296.), Side::Left);
    assert_eq!(tiny.width(px(0.)), px(320.));
}

#[test]
fn at_1100_px_and_below_the_sheet_takes_the_passive_card_width() {
    let at_limit = SheetFrame::new(px(1100.), px(546.), Side::Left);
    assert!(at_limit.is_narrow());
    assert_eq!(at_limit.width(px(0.)), px(546.));
    let above = SheetFrame::new(px(1101.), px(546.5), Side::Left);
    assert!(!above.is_narrow());
    assert_eq!(above.width(px(0.)), px(462.));
}

#[test]
fn keyboard_steps_move_48_px_and_stop_at_the_clamps() {
    let wide = frame(1440., Side::Left);
    let mut state = SheetState::new();
    state.step(Step::Wider, &wide);
    assert_eq!(state.width(&wide), px(646.));
    state.step(Step::Narrower, &wide);
    state.step(Step::Narrower, &wide);
    assert_eq!(state.width(&wide), px(550.));
    for _ in 0..20 {
        state.step(Step::Wider, &wide);
    }
    assert_eq!(state.width(&wide), px(1064.));
    state.step(Step::Narrower, &wide);
    assert_eq!(state.width(&wide), px(1016.));
}

#[test]
fn the_width_is_kept_as_a_delta_from_the_default() {
    let wide = frame(1440., Side::Left);
    let mut state = SheetState::new();
    state.step(Step::Wider, &wide);
    assert_eq!(state.delta(), px(48.));
    let wider_window = frame(1920., Side::Left);
    assert_eq!(state.width(&wider_window), px(800. + 48.));
}

#[gpui_kit::test]
fn the_rendered_sheet_covers_the_passive_side_and_jumps_with_the_active_pane(
    cx: &mut TestAppContext,
) {
    let rig = open(cx, frame(1440., Side::Left));
    rig.settle(cx);
    let right = rig.sheet(cx);
    assert_eq!(right.right(), px(1424.));
    assert_eq!(right.size.width, px(598.));
    rig.set_frame(frame(1440., Side::Right), cx);
    rig.settle(cx);
    let left = rig.sheet(cx);
    assert_eq!(left.left(), px(0.));
    assert_eq!(left.size.width, px(598.));
}

#[gpui_kit::test]
fn at_1100_px_the_rendered_sheet_lies_exactly_over_the_passive_card(cx: &mut TestAppContext) {
    let narrow = frame(1100., Side::Left);
    let rig = open(cx, narrow);
    rig.settle(cx);
    let sheet = rig.sheet(cx);
    let passive_card_left = narrow.body - narrow.passive_card;
    assert_eq!(sheet.left(), passive_card_left);
    assert_eq!(sheet.right(), narrow.body);
    rig.set_frame(frame(1100., Side::Right), cx);
    rig.settle(cx);
    let sheet = rig.sheet(cx);
    assert_eq!(sheet.left(), px(0.));
    assert_eq!(sheet.right(), narrow.passive_card);
}
