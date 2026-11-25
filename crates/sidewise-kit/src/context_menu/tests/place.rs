use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, TestAppContext, point, px, size};

use super::{open_menu, window_bounds};
use crate::context_menu::aim::is_aiming;
use crate::context_menu::place::{
    Flyout, Panel, Side, chrome, flyout_frame, opening_origin, panel_frame, place_root,
};

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

#[test]
fn a_bottom_anchored_menu_shifts_up_when_async_items_grow_it() {
    let area = Bounds::new(point(px(0.), px(0.)), size(px(1440.), px(900.)));
    let chrome_h = chrome();
    let initial = Panel {
        origin: point(px(400.), px(620.)),
        width: px(316.),
        list: 260. - chrome_h,
    };
    let origin = opening_origin(initial, initial.origin, area);
    assert_eq!(origin.y, px(620.));
    let expanded = Panel {
        origin,
        width: px(316.),
        list: 520. - chrome_h,
    };
    let frame = panel_frame(expanded, area);
    assert_eq!(frame.bounds.size.height, px(520.));
    assert_eq!(frame.bounds.top(), px(372.));
    assert_eq!(frame.bounds.bottom(), px(892.));
}

#[gpui_kit::test]
fn shortcut_hints_render_as_right_aligned_keycaps_inside_their_rows(cx: &mut TestAppContext) {
    let menu = super::open_default(cx);
    assert_eq!(menu.find(("menu-key", 6), cx).as_deref(), Some("Ctrl+X"));
    assert_eq!(menu.find(("menu-key", 2), cx), None);
    let row = menu.row_bounds(6, cx);
    let cap = cx
        .update_window(menu.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(super::ROOT)
                .within(("menu-level", 0usize))
                .find(("menu-key", 6usize))
                .bounds()
        })
        .expect("window");
    assert!(cap.left() > row.center().x, "{row:?} {cap:?}");
    assert!(cap.right() <= row.right(), "{row:?} {cap:?}");
    assert!(cap.top() >= row.top() && cap.bottom() <= row.bottom());
}

#[gpui_kit::test]
fn rendered_menu_near_bottom_shifts_up_when_async_items_arrive(cx: &mut TestAppContext) {
    use navo_core::shell_menu::{Group, ShellMenu, navo_actions};
    let area = Bounds::new(point(px(0.), px(0.)), size(px(1200.), px(900.)));
    let initial = ShellMenu {
        items: navo_actions().into_iter().take(5).collect(),
    };
    let menu = super::open_menu_in(cx, (point(px(300.), px(640.)), area), initial);
    let before = menu.level_bounds(cx);
    assert_eq!(before.top(), px(640.));
    let mut expanded_items = navo_actions();
    expanded_items.push(navo_core::shell_menu::MenuItem::separator(Group::Navo));
    for i in 0..8 {
        expanded_items.push(super::shell(
            100 + i,
            &format!("Shell action {i}"),
            Group::Extensions,
        ));
    }
    menu.menu.update(cx, |m, cx| {
        m.set_menu(
            ShellMenu {
                items: expanded_items,
            },
            cx,
        );
    });
    menu.frame(cx);
    let after = menu.level_bounds(cx);
    assert!(after.top() < before.top(), "{before:?} {after:?}");
    assert!(after.top() >= px(8.), "{after:?}");
    assert_eq!(after.bottom(), px(892.));
    assert!(
        after.size.height > before.size.height,
        "{before:?} {after:?}"
    );
}
