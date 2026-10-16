use gpui_kit::TestAppContext;
use rusty_core::shell_menu::{Group, Submenu};

use super::{ContextMenuEvent, SEND_TO, open_default, shell, zip_menu};

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
    menu.hover(0, 15, cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("7-Zip"));
    assert!(!menu.has_level(1, cx));
    menu.wait(200, cx);
    assert_eq!(menu.rows(1, cx)[0], "Open archive");
}

#[gpui_kit::test]
fn a_lazy_submenu_is_requested_once_while_it_loads(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right", "left", "right"], cx);
    assert_eq!(
        menu.events(),
        [ContextMenuEvent::OpenSubmenu(vec![SEND_TO])]
    );
    assert_eq!(menu.rows(1, cx), ["Loading…"]);
}

fn with_icons() -> Submenu {
    let icon = rusty_core::shell_menu::Icon {
        width: 2,
        height: 2,
        rgba: vec![255; 16],
    };
    Submenu::Filled(
        (0..3)
            .map(|index| rusty_core::shell_menu::MenuItem {
                icon: Some(icon.clone()),
                ..shell(60 + index, &format!("Target {index}"), Group::Share)
            })
            .collect(),
    )
}

#[gpui_kit::test]
fn icons_stay_bounded_when_a_submenu_is_filled_again(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    for _ in 0..4 {
        menu.menu.update(cx, |menu, cx| {
            menu.set_submenu(&[SEND_TO], with_icons(), cx);
        });
    }
    assert_eq!(menu.menu.read_with(cx, |menu, _| menu.icons.len()), 3);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    assert_eq!(menu.menu.read_with(cx, |menu, _| menu.icons.len()), 0);
}

#[gpui_kit::test]
fn a_new_menu_merges_in_and_keeps_the_selected_row(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down"], cx);
    let mut bigger = zip_menu();
    bigger.items.push(shell(99, "Pin to Start", Group::Open));
    menu.menu.update(cx, |menu, cx| menu.set_menu(bigger, cx));
    menu.frame(cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("View"));
    assert!(menu.rows(0, cx).iter().any(|row| row == "Pin to Start"));
}
