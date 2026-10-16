use gpui_kit::TestAppContext;
use rusty_core::shell_menu::{FerriteAction, ItemId};

use super::{ContextMenuEvent, open_default};

fn shell(command: u32) -> ContextMenuEvent {
    ContextMenuEvent::Chosen(ItemId::Shell {
        command,
        verb: None,
    })
}

#[gpui_kit::test]
fn arrows_skip_headers_separators_and_disabled_rows(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Copy path"));
    menu.press(&["down", "down", "down"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Open in other pane"));
    menu.press(&["down", "down", "down"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Open"));
    menu.press(&["down", "down"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("7-Zip"));
    menu.press(&["up", "up", "up"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Classic menu"));
}

#[gpui_kit::test]
fn arrows_wrap_and_home_end_jump(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["up"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("More (2)"));
    menu.press(&["down"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Copy path"));
    menu.press(&["end"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("More (2)"));
    menu.press(&["home"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Copy path"));
    menu.press(&["pagedown"], cx);
    assert_ne!(menu.selected(0, cx).as_deref(), Some("Copy path"));
    menu.press(&["pageup"], cx);
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Copy path"));
}

#[gpui_kit::test]
fn enter_emits_the_chosen_command(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down", "down", "enter"], cx);
    assert_eq!(
        menu.events(),
        [ContextMenuEvent::Chosen(ItemId::Ferrite(
            FerriteAction::Rename
        ))]
    );
}

#[gpui_kit::test]
fn enter_on_a_shell_verb_emits_its_command(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("cut", cx);
    menu.press(&["enter"], cx);
    assert_eq!(menu.events(), [shell(7)]);
}

#[gpui_kit::test]
fn classic_menu_asks_for_the_native_menu(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["shift-f10"], cx);
    menu.type_text("classic", cx);
    menu.press(&["enter"], cx);
    assert_eq!(
        menu.events(),
        [ContextMenuEvent::ShowClassic, ContextMenuEvent::ShowClassic]
    );
}

#[gpui_kit::test]
fn disabled_rows_never_emit(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(0, 4, cx);
    menu.click(0, 18, cx);
    assert!(menu.events().is_empty());
    menu.type_text("paste", cx);
    assert_eq!(menu.rows(0, cx), ["Paste"]);
    menu.press(&["enter"], cx);
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn a_click_runs_the_row(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(0, 9, cx);
    assert_eq!(menu.events(), [shell(1)]);
}
