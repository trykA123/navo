use gpui_kit::{TestAppContext, point, px};
use rusty_core::shell_menu::{ShellMenu, ferrite_actions};

use super::{open_menu_with, zip_menu};

fn ferrite_only() -> ShellMenu {
    ShellMenu {
        items: ferrite_actions(),
    }
}

fn reduce_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

#[gpui_kit::test]
fn the_menu_rises_four_pixels_into_place(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ferrite_only());
    assert_eq!(menu.level_bounds(0, cx).origin.y, px(24.));
    menu.wait(60, cx);
    let halfway = menu.level_bounds(0, cx).origin.y;
    assert!(halfway > px(20.) && halfway < px(24.), "{halfway:?}");
    menu.wait(100, cx);
    assert_eq!(menu.level_bounds(0, cx).origin.y, px(20.));
}

#[gpui_kit::test]
fn reduced_motion_opens_without_movement(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ferrite_only());
    reduce_motion(cx);
    assert_eq!(menu.level_bounds(0, cx).origin.y, px(20.));
    menu.wait(60, cx);
    assert_eq!(menu.level_bounds(0, cx).origin.y, px(20.));
}

#[gpui_kit::test]
fn new_items_grow_the_menu_downward_without_moving_its_rows(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ferrite_only());
    menu.wait(200, cx);
    let before = menu.level_bounds(0, cx);
    let open_row = menu.row_bounds(1, cx);
    menu.menu
        .update(cx, |menu, cx| menu.set_menu(zip_menu(), cx));
    menu.frame(cx);
    assert_eq!(menu.level_bounds(0, cx), before);
    menu.wait(75, cx);
    let growing = menu.level_bounds(0, cx);
    assert_eq!(growing.origin, before.origin);
    assert!(growing.size.height > before.size.height, "{growing:?}");
    assert_eq!(menu.row_bounds(1, cx), open_row);
    menu.wait(200, cx);
    let grown = menu.level_bounds(0, cx);
    assert_eq!(grown.origin, before.origin);
    assert!(grown.size.height > growing.size.height, "{grown:?}");
    assert_eq!(menu.row_bounds(1, cx), open_row);
}

#[gpui_kit::test]
fn reduced_motion_grows_at_once(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ferrite_only());
    reduce_motion(cx);
    let before = menu.level_bounds(0, cx);
    menu.menu
        .update(cx, |menu, cx| menu.set_menu(zip_menu(), cx));
    menu.frame(cx);
    let after = menu.level_bounds(0, cx);
    assert_eq!(after.origin, before.origin);
    assert!(after.size.height > before.size.height, "{after:?}");
}

#[gpui_kit::test]
fn a_menu_placed_above_its_anchor_grows_without_moving(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(560.)), ferrite_only());
    reduce_motion(cx);
    let before = menu.level_bounds(0, cx);
    let open_row = menu.row_bounds(1, cx);
    menu.menu
        .update(cx, |menu, cx| menu.set_menu(zip_menu(), cx));
    menu.frame(cx);
    assert_eq!(menu.level_bounds(0, cx).origin, before.origin);
    assert_eq!(menu.row_bounds(1, cx), open_row);
}
