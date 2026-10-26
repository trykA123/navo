use gpui_kit::{Bounds, TestAppContext, point, px, size};
use rusty_core::shell_menu::{Group, MenuItem, ShellMenu};

use super::{filled, open_menu, open_menu_in, shell, window_bounds};
use crate::context_menu::place::place_root;

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

fn nested(depth: u32) -> MenuItem {
    let long = "A long submenu label that asks for a much wider panel than the root";
    let children = if depth == 0 {
        (0..12)
            .map(|index| shell(100 + index, &format!("{long} {index}"), Group::Other))
            .collect()
    } else {
        vec![nested(depth - 1), shell(90 + depth, "Leaf", Group::Other)]
    };
    filled(80 + depth, &format!("Level {depth}"), children)
}

#[gpui_kit::test]
fn four_drills_deep_the_panel_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
    let area = Bounds::new(point(px(0.), px(0.)), size(px(390.), px(600.)));
    let menu = ShellMenu {
        items: vec![nested(3), shell(1, "Open", Group::Open)],
    };
    let opened = open_menu_in(cx, (point(px(300.), px(500.)), area), menu);
    opened.wait(300, cx);
    let root = opened.level_bounds(cx);
    for _ in 0..4 {
        opened.press(&["right"], cx);
        opened.wait(300, cx);
        let bounds = opened.level_bounds(cx);
        assert!(bounds.left() >= px(0.), "{bounds:?}");
        assert!(bounds.right() <= px(390.), "{bounds:?}");
        assert!(bounds.top() >= px(0.), "{bounds:?}");
        assert!(bounds.bottom() <= px(600.), "{bounds:?}");
    }
    assert_eq!(opened.back_row(cx).as_deref(), Some("Level 0"));
    assert!(opened.level_bounds(cx).size.width >= root.size.width);
}

#[gpui_kit::test]
fn the_panel_keeps_its_top_left_and_widest_width(cx: &mut TestAppContext) {
    let menu = ShellMenu {
        items: vec![nested(1), shell(1, "Open", Group::Open)],
    };
    let opened = super::open_menu_with(cx, point(px(40.), px(20.)), menu);
    opened.wait(300, cx);
    let root = opened.level_bounds(cx);
    opened.press(&["right", "right"], cx);
    opened.wait(300, cx);
    let page = opened.level_bounds(cx);
    assert_eq!(page.origin, root.origin);
    assert!(page.size.width > root.size.width, "{page:?}");
    opened.press(&["left", "left"], cx);
    opened.wait(300, cx);
    assert_eq!(opened.level_bounds(cx).size.width, page.size.width);
}
