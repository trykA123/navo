use std::time::{Duration, Instant};

use gpui_kit::{TestAppContext, point, px};
use rusty_core::shell_menu::{ShellMenu, ferrite_actions};

use super::{open_menu_with, zip_menu};
use crate::context_menu::motion::progress;

fn ferrite_only() -> ShellMenu {
    ShellMenu {
        items: ferrite_actions(),
    }
}

fn grown() -> ShellMenu {
    let mut items = ferrite_actions();
    items.extend(zip_menu().items);
    ShellMenu { items }
}

#[test]
fn the_menu_fades_in_within_the_fade_time() {
    let fade = Duration::from_millis(80);
    let opened = Instant::now();
    assert_eq!(progress(opened, opened, fade), 0.);
    let halfway = progress(opened, opened + fade / 2, fade);
    assert!(halfway > 0. && halfway < 1., "{halfway}");
    assert_eq!(progress(opened, opened + fade, fade), 1.);
}

#[gpui_kit::test]
fn the_menu_opens_in_place_on_its_first_frame(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ferrite_only());
    assert_eq!(menu.level_bounds(cx).origin.y, px(20.));
    menu.wait(40, cx);
    assert_eq!(menu.level_bounds(cx).origin.y, px(20.));
}

#[gpui_kit::test]
fn new_items_show_at_full_height_at_once_without_moving_rows(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ferrite_only());
    menu.wait(200, cx);
    let before = menu.level_bounds(cx);
    let open_row = menu.row_bounds(1, cx);
    menu.menu.update(cx, |menu, cx| menu.set_menu(grown(), cx));
    menu.frame(cx);
    let after = menu.level_bounds(cx);
    assert_eq!(after.origin, before.origin);
    assert!(after.size.height > before.size.height, "{after:?}");
    assert_eq!(menu.row_bounds(1, cx), open_row);
    menu.wait(200, cx);
    assert_eq!(menu.level_bounds(cx), after);
}

#[gpui_kit::test]
fn a_menu_placed_above_its_anchor_grows_without_moving(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(560.)), ferrite_only());
    let before = menu.level_bounds(cx);
    let open_row = menu.row_bounds(1, cx);
    menu.menu.update(cx, |menu, cx| menu.set_menu(grown(), cx));
    menu.frame(cx);
    assert_eq!(menu.level_bounds(cx).origin, before.origin);
    assert_eq!(menu.row_bounds(1, cx), open_row);
}

#[gpui_kit::test]
fn the_root_stays_still_while_a_flyout_opens_and_closes(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), zip_menu());
    menu.wait(200, cx);
    let resting = menu.level_bounds(cx);
    menu.press(&["down", "down", "right"], cx);
    assert_eq!(menu.level_bounds(cx), resting);
    menu.press(&["left"], cx);
    assert_eq!(menu.level_bounds(cx), resting);
}

#[gpui_kit::test]
fn a_flyout_shows_in_place_on_its_first_frame(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), zip_menu());
    menu.press(&["down", "down", "right"], cx);
    let first = menu.level_bounds_in(1, cx);
    menu.wait(40, cx);
    assert_eq!(menu.level_bounds_in(1, cx), first);
}
