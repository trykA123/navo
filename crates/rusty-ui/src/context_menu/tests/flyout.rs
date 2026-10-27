use gpui_kit::{Bounds, TestAppContext, point, px, size};
use rusty_core::shell_menu::{Group, ItemId, ItemKind, ShellMenu, Submenu};

use super::{
    ContextMenuEvent, ROOT_ROWS, SEND_TO, SEVEN_ZIP, filled, open_default, open_menu_in,
    open_menu_with, shell, zip_menu,
};

pub(super) const SEVEN_ZIP_ROWS: [&str; 5] = [
    "Open archive",
    "Extract files…",
    "Extract Here",
    "-",
    "Extract to \"dist\\\"",
];

pub(super) fn send_targets() -> Submenu {
    Submenu::Filled(vec![
        shell(50, "Desktop (create shortcut)", Group::Share),
        shell(51, "Documents", Group::Share),
        shell(52, "Mail recipient", Group::Share),
    ])
}

/// The zip menu with a "CRC SHA" submenu inside 7-Zip, at row 5 of the 7-Zip flyout.
pub(super) fn nested_menu() -> ShellMenu {
    let deep = filled(40, "CRC SHA", vec![shell(41, "SHA-256", Group::Extensions)]);
    let mut items = zip_menu().items;
    let ItemKind::Submenu(Submenu::Filled(children)) = &mut items[SEVEN_ZIP].kind else {
        panic!("7-Zip is filled");
    };
    children.push(deep);
    ShellMenu { items }
}

fn on_seven_zip(cx: &mut TestAppContext) -> super::Opened {
    let menu = open_default(cx);
    menu.press(&["down", "down", "right"], cx);
    menu
}

#[gpui_kit::test]
fn right_opens_a_flyout_beside_the_root_and_focuses_its_first_row(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    assert_eq!(menu.rows(cx), ROOT_ROWS);
    assert_eq!(menu.rows_in(1, cx), SEVEN_ZIP_ROWS);
    assert_eq!(menu.selected_in(1, cx).as_deref(), Some("Open archive"));
    let (root, flyout) = (menu.level_bounds(cx), menu.level_bounds_in(1, cx));
    assert!(
        flyout.left() >= root.right() - px(4.),
        "{root:?} {flyout:?}"
    );
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn the_flyout_lines_up_with_the_row_that_opened_it(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    let parent = menu.row_bounds(2, cx);
    let first = menu.row_bounds_in(1, 0, cx);
    assert_eq!(first.top(), parent.top());
}

#[gpui_kit::test]
fn enter_opens_and_focuses_like_right(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down", "down", "enter"], cx);
    assert_eq!(menu.selected_in(1, cx).as_deref(), Some("Open archive"));
    menu.press(&["down", "down", "enter"], cx);
    let here = ItemId::Shell {
        command: 32,
        verb: None,
    };
    assert_eq!(menu.events(), [ContextMenuEvent::Chosen(here)]);
}

#[gpui_kit::test]
fn left_and_backspace_close_the_flyout_and_return_to_its_row(cx: &mut TestAppContext) {
    for key in ["left", "backspace"] {
        let menu = on_seven_zip(cx);
        menu.press(&["down", key], cx);
        assert_eq!(menu.levels(cx), 1, "{key}");
        assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"), "{key}");
        menu.press(&["down"], cx);
        assert_eq!(menu.selected(cx).as_deref(), Some("Send to"), "{key}");
    }
}

#[gpui_kit::test]
fn escape_closes_the_innermost_flyout_then_the_menu(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), nested_menu());
    menu.press(&["down", "down", "right", "end", "right"], cx);
    assert_eq!(menu.rows_in(2, cx), ["SHA-256"]);
    menu.press(&["escape"], cx);
    assert_eq!(menu.levels(cx), 2);
    assert_eq!(menu.selected_in(1, cx).as_deref(), Some("CRC SHA"));
    menu.press(&["escape"], cx);
    assert_eq!(menu.levels(cx), 1);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
    assert!(menu.events().is_empty());
    menu.press(&["escape"], cx);
    assert_eq!(menu.events(), [ContextMenuEvent::Dismissed]);
}

#[gpui_kit::test]
fn arrows_in_the_root_close_an_open_flyout(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(2, cx);
    assert_eq!(menu.levels(cx), 2);
    menu.press(&["down"], cx);
    assert_eq!(menu.levels(cx), 1);
    assert_eq!(menu.selected(cx).as_deref(), Some("Send to"));
}

#[gpui_kit::test]
fn a_click_on_a_submenu_row_opens_its_flyout_at_once(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(2, cx);
    assert_eq!(menu.rows_in(1, cx), SEVEN_ZIP_ROWS);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn a_click_in_a_flyout_runs_its_command(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(2, cx);
    menu.click_in(1, 2, cx);
    let here = ItemId::Shell {
        command: 32,
        verb: None,
    };
    assert_eq!(menu.events(), [ContextMenuEvent::Chosen(here)]);
}

#[gpui_kit::test]
fn a_nested_submenu_opens_a_further_flyout(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), nested_menu());
    menu.click(2, cx);
    menu.click_in(1, 5, cx);
    assert_eq!(menu.rows_in(2, cx), ["SHA-256"]);
    let (one, two) = (menu.level_bounds_in(1, cx), menu.level_bounds_in(2, cx));
    assert!(two.left() >= one.right() - px(4.), "{one:?} {two:?}");
}

#[gpui_kit::test]
fn the_flyout_flips_left_when_the_right_has_no_room(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(460.), px(20.)), zip_menu());
    menu.press(&["down", "down", "right"], cx);
    let (root, flyout) = (menu.level_bounds(cx), menu.level_bounds_in(1, cx));
    assert!(root.right() <= px(792.), "{root:?}");
    assert!(
        flyout.right() <= root.left() + px(4.),
        "{root:?} {flyout:?}"
    );
    assert!(flyout.left() >= px(8.), "{flyout:?}");
}

#[gpui_kit::test]
fn the_flyout_shifts_up_to_stay_in_the_window(cx: &mut TestAppContext) {
    let children = (0..8)
        .map(|index| shell(60 + index, &format!("Target {index}"), Group::Share))
        .collect();
    let menu = ShellMenu {
        items: vec![
            shell(1, "Open", Group::Open),
            filled(3, "Targets", children),
        ],
    };
    let menu = open_menu_with(cx, point(px(40.), px(540.)), menu);
    menu.press(&["down", "right"], cx);
    let parent = menu.row_bounds(1, cx);
    let flyout = menu.level_bounds_in(1, cx);
    assert!(flyout.bottom() <= px(592.), "{flyout:?}");
    assert!(flyout.top() < parent.top(), "{parent:?} {flyout:?}");
}

#[gpui_kit::test]
fn three_flyouts_deep_every_panel_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
    let area = Bounds::new(point(px(0.), px(0.)), size(px(390.), px(600.)));
    let menu = open_menu_in(cx, (point(px(300.), px(500.)), area), nested_menu());
    menu.press(&["down", "down", "right", "end", "right"], cx);
    for level in 0..3 {
        let bounds = menu.level_bounds_in(level, cx);
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(390.),
            "{bounds:?}"
        );
        assert!(
            bounds.top() >= px(0.) && bounds.bottom() <= px(600.),
            "{bounds:?}"
        );
    }
}

#[gpui_kit::test]
fn typing_closes_flyouts_and_filters_the_root(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.type_text("cut", cx);
    assert_eq!(menu.levels(cx), 1);
    assert_eq!(menu.rows(cx), ["Cut", "Create shortcut"]);
}

#[gpui_kit::test]
fn a_filtered_submenu_row_opens_as_a_flyout_and_keeps_the_filter(cx: &mut TestAppContext) {
    let menu = open_menu_with(cx, point(px(40.), px(20.)), nested_menu());
    menu.type_text("crc", cx);
    assert_eq!(menu.rows(cx), ["7-Zip › CRC SHA"]);
    menu.press(&["enter"], cx);
    assert_eq!(menu.filter_text(cx), "crc");
    assert_eq!(menu.rows_in(1, cx), ["SHA-256"]);
    menu.press(&["left"], cx);
    assert_eq!(menu.levels(cx), 1);
    assert_eq!(menu.filter_text(cx), "crc");
}

#[gpui_kit::test]
fn a_lazy_flyout_asks_once_and_fills_in_place(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right", "left", "right"], cx);
    assert_eq!(
        menu.events(),
        [ContextMenuEvent::OpenSubmenu(vec![SEND_TO])]
    );
    menu.menu.update(cx, |menu, cx| {
        assert!(menu.set_submenu(&[SEND_TO], send_targets(), cx));
    });
    menu.frame(cx);
    assert_eq!(
        menu.rows_in(1, cx),
        ["Desktop (create shortcut)", "Documents", "Mail recipient"]
    );
    menu.press(&["down", "enter"], cx);
    let documents = ItemId::Shell {
        command: 51,
        verb: None,
    };
    assert_eq!(
        menu.events().last(),
        Some(&ContextMenuEvent::Chosen(documents))
    );
}

#[gpui_kit::test]
fn a_flyout_whose_row_leaves_the_menu_closes(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    let mut items = zip_menu().items;
    items[SEVEN_ZIP].hidden = true;
    menu.menu
        .update(cx, |menu, cx| menu.set_menu(ShellMenu { items }, cx));
    assert_eq!(menu.levels(cx), 1);
    menu.press(&["down"], cx);
    assert!(menu.selected(cx).is_some());
}

#[gpui_kit::test]
fn a_failed_flyout_offers_a_retry(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right"], cx);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    menu.frame(cx);
    assert_eq!(menu.rows_in(1, cx), ["Could not load", "Retry"]);
    menu.press(&["enter"], cx);
    menu.click_in(1, 1, cx);
    assert_eq!(
        menu.events(),
        vec![ContextMenuEvent::OpenSubmenu(vec![SEND_TO]); 3]
    );
}

#[gpui_kit::test]
fn reopening_a_failed_flyout_loads_it_again(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right"], cx);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    menu.press(&["left", "right"], cx);
    assert_eq!(
        menu.events(),
        vec![ContextMenuEvent::OpenSubmenu(vec![SEND_TO]); 2]
    );
    menu.wait(200, cx);
    assert_eq!(menu.rows_in(1, cx), ["Loading…"]);
}

#[gpui_kit::test]
fn mouse_button_four_closes_the_innermost_flyout(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.mouse_back(cx);
    assert_eq!(menu.levels(cx), 1);
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn shift_f10_in_a_flyout_asks_for_the_native_menu(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.press(&["shift-f10"], cx);
    assert_eq!(menu.events(), [ContextMenuEvent::ShowClassic]);
}

#[gpui_kit::test]
fn right_on_a_command_row_moves_the_filter_caret(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("ext", cx);
    menu.press(&["left", "left"], cx);
    menu.type_text("X", cx);
    menu.press(&["right"], cx);
    menu.type_text("Y", cx);
    assert_eq!(menu.filter_text(cx), "eXxYt");
}
