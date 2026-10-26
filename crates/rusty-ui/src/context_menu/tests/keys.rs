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
fn arrows_skip_separators_and_disabled_rows(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Open"));
    menu.press(&["down", "down", "down"], cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Send to"));
    menu.press(&["down", "down", "down"], cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Create shortcut"));
    menu.press(&["up"], cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Copy"));
}

#[gpui_kit::test]
fn arrows_wrap_and_home_end_jump(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["up"], cx);
    assert_eq!(
        menu.selected(cx).as_deref(),
        Some("Restore previous versions")
    );
    menu.press(&["down"], cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Open"));
    menu.press(&["end"], cx);
    assert_eq!(
        menu.selected(cx).as_deref(),
        Some("Restore previous versions")
    );
    menu.press(&["home"], cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Open"));
    menu.press(&["pagedown"], cx);
    assert_ne!(menu.selected(cx).as_deref(), Some("Open"));
    menu.press(&["pageup"], cx);
    assert_eq!(menu.selected(cx).as_deref(), Some("Open"));
}

#[gpui_kit::test]
fn enter_emits_the_chosen_command(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["enter"], cx);
    assert_eq!(menu.events(), [shell(1)]);
}

#[gpui_kit::test]
fn a_hidden_ferrite_action_runs_when_typed(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("rename", cx);
    assert_eq!(menu.rows(cx), ["Rename"]);
    menu.press(&["enter"], cx);
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
    let by_key = open_default(cx);
    by_key.press(&["shift-f10"], cx);
    assert_eq!(by_key.events(), [ContextMenuEvent::ShowClassic]);
    let by_row = open_default(cx);
    by_row.type_text("classic", cx);
    by_row.press(&["enter"], cx);
    assert_eq!(by_row.events(), [ContextMenuEvent::ShowClassic]);
}

#[gpui_kit::test]
fn shift_f10_on_a_page_asks_for_the_native_menu(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down", "down", "right", "shift-f10"], cx);
    assert_eq!(menu.events(), [ContextMenuEvent::ShowClassic]);
}

#[gpui_kit::test]
fn a_menu_without_classic_hides_it_and_ignores_shift_f10(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.menu.update(cx, |menu, cx| menu.hide_classic(cx));
    menu.frame(cx);
    assert!(!menu.has_classic(cx));
    menu.press(&["shift-f10"], cx);
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn disabled_rows_never_emit(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(8, cx);
    assert!(menu.events().is_empty());
    menu.type_text("paste", cx);
    assert_eq!(menu.rows(cx), ["Paste"]);
    menu.press(&["enter"], cx);
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn a_click_runs_the_row(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.click(6, cx);
    assert_eq!(menu.events(), [shell(7)]);
}

#[gpui_kit::test]
fn closing_the_menu_gives_focus_back(cx: &mut TestAppContext) {
    for keys in [&["escape"][..], &["enter"], &["shift-f10"]] {
        let menu = open_default(cx);
        assert!(!menu.restore_is_focused(cx));
        menu.press(keys, cx);
        assert_eq!(menu.events().len(), 1);
        assert!(menu.restore_is_focused(cx), "{keys:?}");
    }
}
