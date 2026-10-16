use gpui_kit::TestAppContext;
use rusty_core::shell_menu::{Group, Submenu};

use super::{ContextMenuEvent, SEND_TO, open_default, shell};

fn send_targets() -> Submenu {
    Submenu::Filled(vec![
        shell(50, "Desktop (create shortcut)", Group::Share),
        shell(51, "Documents", Group::Share),
        shell(52, "Mail recipient", Group::Share),
    ])
}

#[gpui_kit::test]
fn right_on_a_lazy_submenu_asks_for_it_and_shows_it_when_filled(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Send to"));
    menu.press(&["right"], cx);
    assert_eq!(
        menu.events(),
        [ContextMenuEvent::OpenSubmenu(vec![SEND_TO])]
    );
    assert_eq!(menu.rows(1, cx), ["Loading…"]);
    menu.menu.update(cx, |menu, cx| {
        assert!(menu.set_submenu(&[SEND_TO], send_targets(), cx));
    });
    menu.frame(cx);
    assert_eq!(
        menu.rows(1, cx),
        ["Desktop (create shortcut)", "Documents", "Mail recipient"]
    );
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Send to"));
    assert_eq!(
        menu.selected(1, cx).as_deref(),
        Some("Desktop (create shortcut)")
    );
    menu.press(&["down", "enter"], cx);
    assert_eq!(
        menu.events().last(),
        Some(&ContextMenuEvent::Chosen(
            rusty_core::shell_menu::ItemId::Shell {
                command: 51,
                verb: None
            }
        ))
    );
}

#[gpui_kit::test]
fn a_failed_submenu_offers_a_retry(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right"], cx);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    menu.frame(cx);
    assert_eq!(menu.rows(1, cx), ["Could not load", "Retry"]);
    menu.press(&["enter"], cx);
    assert_eq!(
        menu.events(),
        [
            ContextMenuEvent::OpenSubmenu(vec![SEND_TO]),
            ContextMenuEvent::OpenSubmenu(vec![SEND_TO]),
        ]
    );
}

#[gpui_kit::test]
fn left_closes_the_submenu_and_keeps_the_parent_selected(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("7-zip", cx);
    menu.press(&["right"], cx);
    assert_eq!(menu.selected(1, cx).as_deref(), Some("Open archive"));
    menu.press(&["left"], cx);
    assert!(!menu.has_level(1, cx));
    assert_eq!(menu.selected(0, cx).as_deref(), Some("7-Zip"));
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn hovering_a_submenu_row_opens_it_after_a_pause(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.hover(0, 12, cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("7-Zip"));
    assert!(!menu.has_level(1, cx));
    menu.wait(200, cx);
    assert_eq!(menu.rows(1, cx)[0], "Open archive");
}
