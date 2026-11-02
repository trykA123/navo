use gpui_kit::{Bounds, TestAppContext, point, px, size};

use super::{open_menu, window_bounds};
use crate::context_menu::aim::is_aiming;
use crate::context_menu::place::{Flyout, Side, flyout_frame, place_root};

#[test]
fn the_menu_opens_right_and_down_from_the_anchor_when_it_fits() {
    let origin = place_root(
        point(px(100.), px(50.)),
        size(px(316.), px(300.)),
        window_bounds(),
    );
    assert_eq!(origin, point(px(100.), px(50.)));
}

#[test]
fn the_menu_flips_left_and_up_near_the_bottom_right_corner() {
    let origin = place_root(
        point(px(700.), px(500.)),
        size(px(316.), px(300.)),
        window_bounds(),
    );
    assert_eq!(origin, point(px(384.), px(200.)));
}

#[test]
fn a_menu_taller_than_the_room_above_stays_inside_the_window() {
    let origin = place_root(
        point(px(700.), px(300.)),
        size(px(316.), px(500.)),
        window_bounds(),
    );
    assert_eq!(origin.y, px(92.));
}

#[gpui_kit::test]
fn the_rendered_menu_flips_near_the_window_edge(cx: &mut TestAppContext) {
    let menu = open_menu(cx, point(px(780.), px(590.)));
    let bounds = menu.level_bounds(cx);
    assert!(bounds.right() <= px(780.), "{bounds:?}");
    assert!(bounds.top() >= px(0.), "{bounds:?}");
    assert!(bounds.bottom() <= px(600.), "{bounds:?}");
    assert_eq!(bounds.size.width, px(316.));
}

fn flyout(parent: Bounds<gpui_kit::Pixels>, side: Side) -> Flyout {
    Flyout {
        parent,
        row_top: parent.top() + px(60.),
        width: px(200.),
        list: 140.,
        side,
    }
}

#[test]
fn a_flyout_opens_on_the_right_with_its_first_row_level_with_the_parent_row() {
    let parent = Bounds::new(point(px(100.), px(50.)), size(px(316.), px(300.)));
    let (frame, side) = flyout_frame(flyout(parent, Side::Right), window_bounds());
    assert_eq!(side, Side::Right);
    assert_eq!(frame.bounds.left(), parent.right() - px(2.));
    assert_eq!(frame.bounds.top(), px(105.));
}

#[test]
fn a_flyout_flips_left_at_the_right_edge_and_nested_ones_keep_going_left() {
    let parent = Bounds::new(point(px(470.), px(50.)), size(px(316.), px(300.)));
    let (frame, side) = flyout_frame(flyout(parent, Side::Right), window_bounds());
    assert_eq!(side, Side::Left);
    assert_eq!(frame.bounds.right(), parent.left() + px(2.));
    let (_, nested) = flyout_frame(flyout(frame.bounds, side), window_bounds());
    assert_eq!(nested, Side::Left);
}

#[test]
fn a_flyout_goes_back_right_when_the_left_has_no_room() {
    let parent = Bounds::new(point(px(20.), px(50.)), size(px(316.), px(300.)));
    let (_, side) = flyout_frame(flyout(parent, Side::Left), window_bounds());
    assert_eq!(side, Side::Right);
}

#[test]
fn a_flyout_too_wide_for_either_side_stays_inside_the_window() {
    let area = Bounds::new(point(px(0.), px(0.)), size(px(390.), px(600.)));
    let parent = Bounds::new(point(px(66.), px(50.)), size(px(316.), px(300.)));
    let (frame, _) = flyout_frame(flyout(parent, Side::Right), area);
    assert!(frame.bounds.left() >= px(8.) && frame.bounds.right() <= px(382.));
}

#[test]
fn a_flyout_near_the_bottom_shifts_up() {
    let parent = Bounds::new(point(px(100.), px(400.)), size(px(316.), px(190.)));
    let mut low = flyout(parent, Side::Right);
    low.row_top = px(560.);
    let (frame, _) = flyout_frame(low, window_bounds());
    assert_eq!(frame.bounds.bottom(), px(592.));
}

#[test]
fn moving_toward_the_flyout_aims_at_it_and_moving_along_the_menu_does_not() {
    let target = Bounds::new(point(px(400.), px(100.)), size(px(200.), px(200.)));
    let from = point(px(300.), px(150.));
    assert!(is_aiming(
        from,
        point(px(350.), px(170.)),
        target,
        Side::Right
    ));
    assert!(!is_aiming(
        from,
        point(px(300.), px(180.)),
        target,
        Side::Right
    ));
    assert!(!is_aiming(
        from,
        point(px(250.), px(150.)),
        target,
        Side::Right
    ));
    assert!(!is_aiming(from, from, target, Side::Right));
}

#[test]
fn a_pointer_inside_the_overlap_strip_still_aims_at_the_flyout_edge() {
    let target = Bounds::new(point(px(400.), px(100.)), size(px(200.), px(200.)));
    let from = point(px(401.), px(150.));
    assert!(is_aiming(
        from,
        point(px(400.5), px(160.)),
        target,
        Side::Right
    ));
    let left = Bounds::new(point(px(100.), px(100.)), size(px(200.), px(200.)));
    let from = point(px(299.), px(150.));
    assert!(is_aiming(
        from,
        point(px(299.5), px(160.)),
        left,
        Side::Left
    ));
}
