use gpui_kit::TestAppContext;
use rusty_core::shell_menu::{Group, ItemId, Submenu};

use super::{ContextMenuEvent, ROOT_ROWS, SEND_TO, SEVEN_ZIP, open_default, shell};

fn send_targets() -> Submenu {
    Submenu::Filled(vec![
        shell(50, "Desktop (create shortcut)", Group::Share),
        shell(51, "Documents", Group::Share),
        shell(52, "Mail recipient", Group::Share),
    ])
}

const SEVEN_ZIP_ROWS: [&str; 5] = [
    "Open archive",
    "Extract files…",
    "Extract Here",
    "-",
    "Extract to \"dist\\\"",
];

fn on_seven_zip(cx: &mut TestAppContext) -> super::Opened {
    let menu = open_default(cx);
    menu.press(&["down", "down", "right"], cx);
    menu
}

#[gpui_kit::test]
fn right_drills_into_a_submenu_in_the_same_panel(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
    assert_eq!(menu.rows(cx), SEVEN_ZIP_ROWS);
    assert_eq!(menu.selected(cx).as_deref(), Some("Open archive"));
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn enter_drills_like_right(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down", "down", "enter"], cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn left_and_backspace_go_back_to_the_parent_row(cx: &mut TestAppContext) {
    for key in ["left", "backspace"] {
        let menu = on_seven_zip(cx);
        menu.press(&[key], cx);
        assert_eq!(menu.back_row(cx), None, "{key}");
        assert_eq!(menu.rows(cx), ROOT_ROWS, "{key}");
        assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"), "{key}");
    }
}

#[gpui_kit::test]
fn backspace_edits_the_filter_while_it_holds_text(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.type_text("ex", cx);
    menu.press(&["backspace"], cx);
    assert_eq!(menu.filter_text(cx), "e");
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
    menu.press(&["left"], cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
}

#[gpui_kit::test]
fn escape_clears_then_goes_back_then_closes(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.type_text("here", cx);
    menu.press(&["escape"], cx);
    assert_eq!(menu.filter_text(cx), "");
    assert_eq!(menu.rows(cx), SEVEN_ZIP_ROWS);
    menu.press(&["escape"], cx);
    assert_eq!(menu.back_row(cx), None);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
    assert!(menu.events().is_empty());
    menu.press(&["escape"], cx);
    assert_eq!(menu.events(), [ContextMenuEvent::Dismissed]);
}

#[gpui_kit::test]
fn typing_on_a_page_searches_only_that_page(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.type_text("open", cx);
    assert_eq!(menu.rows(cx), ["Open archive"]);
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
}

#[gpui_kit::test]
fn drilling_from_a_filter_match_clears_the_filter(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("7-z", cx);
    menu.press(&["enter"], cx);
    assert_eq!(menu.filter_text(cx), "");
    assert_eq!(menu.rows(cx), SEVEN_ZIP_ROWS);
}

#[gpui_kit::test]
fn a_command_on_a_page_runs(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.press(&["down", "down", "enter"], cx);
    let here = ItemId::Shell {
        command: 32,
        verb: None,
    };
    assert_eq!(menu.events(), [ContextMenuEvent::Chosen(here)]);
}

#[gpui_kit::test]
fn a_lazy_page_asks_once_and_fills_in_place(cx: &mut TestAppContext) {
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
        menu.rows(cx),
        ["Desktop (create shortcut)", "Documents", "Mail recipient"]
    );
    assert_eq!(menu.back_row(cx).as_deref(), Some("Send to"));
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
fn the_loading_row_waits_before_it_shows(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down", "right"], cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("Open with"));
    assert!(menu.rows(cx).is_empty());
    menu.wait(60, cx);
    assert!(menu.rows(cx).is_empty());
    menu.wait(80, cx);
    assert_eq!(menu.rows(cx), ["Loading…"]);
}

#[gpui_kit::test]
fn a_failed_page_offers_a_retry(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right"], cx);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    menu.frame(cx);
    assert_eq!(menu.rows(cx), ["Could not load", "Retry"]);
    menu.press(&["enter"], cx);
    menu.click(1, cx);
    assert_eq!(
        menu.events(),
        vec![ContextMenuEvent::OpenSubmenu(vec![SEND_TO]); 3]
    );
}

#[gpui_kit::test]
fn hovering_a_submenu_row_opens_nothing(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.hover(2, cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip"));
    menu.wait(1000, cx);
    assert_eq!(menu.back_row(cx), None);
    assert_eq!(menu.rows(cx), ROOT_ROWS);
}

#[gpui_kit::test]
fn a_click_on_a_submenu_row_drills_in(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(2, cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn the_back_row_and_mouse_button_four_go_back(cx: &mut TestAppContext) {
    let menu = on_seven_zip(cx);
    menu.click_back(cx);
    assert_eq!(menu.back_row(cx), None);
    assert_eq!(menu.rows(cx), ROOT_ROWS);
    menu.press(&["right"], cx);
    menu.mouse_back(cx);
    assert_eq!(menu.back_row(cx), None);
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn a_deep_page_shows_its_trail_and_backs_up_one_level(cx: &mut TestAppContext) {
    let deep = super::filled(40, "CRC SHA", vec![shell(41, "SHA-256", Group::Extensions)]);
    let mut items = super::zip_menu().items;
    let rusty_core::shell_menu::ItemKind::Submenu(Submenu::Filled(children)) =
        &mut items[SEVEN_ZIP].kind
    else {
        panic!("7-Zip is filled");
    };
    children.push(deep);
    let menu = super::open_menu_with(
        cx,
        gpui_kit::point(gpui_kit::px(40.), gpui_kit::px(20.)),
        rusty_core::shell_menu::ShellMenu { items },
    );
    menu.type_text("sha", cx);
    menu.press(&["enter"], cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("CRC SHA"));
    menu.press(&["backspace"], cx);
    assert_eq!(menu.back_row(cx).as_deref(), Some("7-Zip"));
    assert_eq!(menu.selected(cx).as_deref(), Some("CRC SHA"));
}

#[gpui_kit::test]
fn a_drilled_page_keeps_the_context_line(cx: &mut TestAppContext) {
    let root = open_default(cx);
    let line = root.find(("menu-context", 0), &mut *cx);
    assert!(line.is_some());
    let menu = on_seven_zip(cx);
    assert_eq!(menu.find(("menu-context", 0), cx), line);
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
