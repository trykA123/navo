use gpui_kit::{Pixels, Point, TestAppContext, point, px};

use rusty_core::shell_menu::{Group, ItemKind, ShellMenu, Submenu};

use super::flyout::{SEVEN_ZIP_ROWS, nested_menu};
use super::{ContextMenuEvent, SEND_TO, SEVEN_ZIP, open_default, open_menu_with, shell, zip_menu};

fn with_seven_zip_open(cx: &mut TestAppContext) -> super::Opened {
    let menu = open_default(cx);
    menu.hover(2, cx);
    menu.wait(300, cx);
    assert_eq!(menu.levels(cx), 2);
    menu
}

#[gpui_kit::test]
fn resting_on_a_submenu_row_opens_its_flyout_after_the_delay(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.hover(2, cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
    menu.wait(200, cx);
    assert_eq!(menu.levels(cx), 1);
    menu.wait(60, cx);
    assert_eq!(menu.rows_in(1, cx), SEVEN_ZIP_ROWS);
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn resting_on_another_row_closes_the_flyout_after_the_delay(cx: &mut TestAppContext) {
    let menu = with_seven_zip_open(cx);
    menu.hover(0, cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Open"));
    menu.wait(200, cx);
    assert_eq!(menu.levels(cx), 2);
    menu.wait(60, cx);
    assert_eq!(menu.levels(cx), 1);
}

#[gpui_kit::test]
fn coming_back_to_the_parent_row_keeps_the_flyout(cx: &mut TestAppContext) {
    let menu = with_seven_zip_open(cx);
    menu.hover(0, cx);
    menu.wait(100, cx);
    menu.hover(2, cx);
    menu.wait(400, cx);
    assert_eq!(menu.rows_in(1, cx), SEVEN_ZIP_ROWS);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
}

fn toward_the_flyout(menu: &super::Opened, cx: &mut TestAppContext) -> Vec<Point<Pixels>> {
    let send_to = menu.row_bounds(4, cx);
    let y = send_to.center().y;
    [40., 20., 6.]
        .into_iter()
        .map(|inset| point(send_to.right() - px(inset), y))
        .collect()
}

#[gpui_kit::test]
fn a_pointer_heading_for_the_flyout_keeps_it_open(cx: &mut TestAppContext) {
    let menu = with_seven_zip_open(cx);
    for position in toward_the_flyout(&menu, cx) {
        menu.move_to(position, cx);
        menu.wait(100, cx);
    }
    assert_eq!(menu.rows_in(1, cx), SEVEN_ZIP_ROWS);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
}

#[gpui_kit::test]
fn a_pointer_that_stops_short_of_the_flyout_switches_rows(cx: &mut TestAppContext) {
    let menu = with_seven_zip_open(cx);
    for position in toward_the_flyout(&menu, cx) {
        menu.move_to(position, cx);
        menu.wait(100, cx);
    }
    menu.wait(300, cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Send to"));
    assert_eq!(
        menu.events(),
        [ContextMenuEvent::OpenSubmenu(vec![SEND_TO])]
    );
}

#[gpui_kit::test]
fn a_hovered_flyout_row_takes_the_keys(cx: &mut TestAppContext) {
    let menu = with_seven_zip_open(cx);
    menu.hover_in(1, 1, cx);
    menu.press(&["down"], cx);
    assert_eq!(menu.selected_in(1, cx).as_deref(), Some("Extract Here"));
    menu.press(&["left"], cx);
    assert_eq!(menu.levels(cx), 1);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
}

#[gpui_kit::test]
fn a_nested_flyout_opens_and_closes_by_hover(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), nested_menu());
    menu.hover(2, cx);
    menu.wait(300, cx);
    menu.hover_in(1, 5, cx);
    menu.wait(300, cx);
    assert_eq!(menu.rows_in(2, cx), ["SHA-256"]);
    menu.hover_in(1, 0, cx);
    menu.wait(300, cx);
    assert_eq!(menu.levels(cx), 2);
}

#[gpui_kit::test]
fn typing_drops_a_pending_hover(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.hover(2, cx);
    menu.type_text("c", cx);
    menu.wait(400, cx);
    assert_eq!(menu.levels(cx), 1);
}

#[gpui_kit::test]
fn a_pointer_leaving_the_menu_cancels_the_pending_hover(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    let row = menu.row_bounds(2, cx);
    menu.move_to(row.center(), cx);
    menu.wait(100, cx);
    menu.move_to(point(px(-10.), px(-10.)), cx);
    menu.wait(400, cx);
    assert_eq!(menu.levels(cx), 1);
    menu.menu
        .read_with(cx, |menu, _| assert!(menu.pointer.is_none()));
}

#[gpui_kit::test]
fn hovering_another_panel_keeps_a_scrolled_flyout_where_it_is(cx: &mut TestAppContext) {
    let mut items = zip_menu().items;
    let ItemKind::Submenu(Submenu::Filled(children)) = &mut items[SEVEN_ZIP].kind else {
        panic!("7-Zip is filled");
    };
    children.extend((0..40).map(|index| shell(100 + index, "Extra", Group::Extensions)));
    let menu = open_menu_with(cx, point(px(40.), px(20.)), ShellMenu { items });
    menu.hover(2, cx);
    menu.wait(300, cx);
    let offset = point(px(0.), px(-30.));
    menu.menu
        .read_with(cx, |menu, _| menu.views[1].scroll.set_offset(offset));
    menu.hover(0, cx);
    menu.wait(100, cx);
    assert_eq!(menu.levels(cx), 2);
    menu.menu.read_with(cx, |menu, _| {
        assert_eq!(menu.views[1].scroll.offset(), offset);
    });
}
