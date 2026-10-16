use gpui_kit::{Bounds, TestAppContext, point, px, size};

use super::{open_menu, window_bounds};
use crate::context_menu::place::{Cascade, Side, place_root, place_submenu};

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

#[test]
fn a_submenu_opens_to_the_right_aligned_to_its_row() {
    let cascade = Cascade {
        parent: Bounds::new(point(px(100.), px(50.)), size(px(316.), px(400.))),
        row_top: px(120.),
        side: Side::Right,
    };
    let placed = place_submenu(cascade, size(px(300.), px(200.)), window_bounds());
    assert_eq!(placed, (point(px(418.), px(115.)), Side::Right));
}

#[test]
fn a_submenu_flips_left_when_the_right_side_has_no_room() {
    let cascade = Cascade {
        parent: Bounds::new(point(px(400.), px(50.)), size(px(316.), px(400.))),
        row_top: px(500.),
        side: Side::Right,
    };
    let placed = place_submenu(cascade, size(px(300.), px(200.)), window_bounds());
    assert_eq!(placed, (point(px(98.), px(392.)), Side::Left));
}

#[gpui_kit::test]
fn the_rendered_menu_flips_near_the_window_edge(cx: &mut TestAppContext) {
    let menu = open_menu(cx, point(px(780.), px(590.)));
    let bounds = menu.level_bounds(0, cx);
    assert!(bounds.right() <= px(780.), "{bounds:?}");
    assert!(bounds.top() >= px(0.), "{bounds:?}");
    assert!(bounds.bottom() <= px(600.), "{bounds:?}");
    assert_eq!(bounds.size.width, px(316.));
}

#[test]
fn a_deeper_cascade_keeps_the_side_it_inherited() {
    let area = Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(720.)));
    let root = Bounds::new(point(px(950.), px(50.)), size(px(316.), px(400.)));
    let sub = size(px(300.), px(200.));
    let first = Cascade {
        parent: root,
        row_top: px(100.),
        side: Side::Right,
    };
    let (one, one_side) = place_submenu(first, sub, area);
    assert_eq!(one_side, Side::Left);
    let level_one = Bounds::new(one, sub);
    let second = Cascade {
        parent: level_one,
        row_top: px(120.),
        side: one_side,
    };
    let (two, two_side) = place_submenu(second, sub, area);
    assert_eq!(two_side, Side::Left);
    assert!(two.x + sub.width <= level_one.left(), "{two:?}");
    assert!(!Bounds::new(two, sub).intersects(&root));
}
