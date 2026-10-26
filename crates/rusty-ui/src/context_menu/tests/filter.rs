use gpui_kit::TestAppContext;
use rusty_core::shell_menu::{Group, MenuItem, ShellMenu};

use super::{ROOT_ROWS, open_default, open_menu_with, shell};

#[gpui_kit::test]
fn the_root_lists_the_shell_items_without_headers_or_ferrite_rows(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    assert_eq!(menu.rows(cx), ROOT_ROWS);
    assert_eq!(
        menu.find(("menu-hint", 0), cx).as_deref(),
        Some("Type to filter")
    );
}

#[gpui_kit::test]
fn typing_narrows_the_rows_to_matches_and_highlights_them(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("ext", cx);
    assert_eq!(
        menu.rows(cx),
        [
            "7-Zip › Extract files…",
            "7-Zip › Extract Here",
            "7-Zip › Extract to \"dist\\\"",
        ]
    );
    assert_eq!(menu.find(("menu-match", 0), cx).as_deref(), Some("Ext"));
    assert_eq!(menu.find(("menu-match", 1), cx).as_deref(), Some("Ext"));
    assert_eq!(menu.selected(cx).as_deref(), Some("7-Zip › Extract files…"));
}

#[gpui_kit::test]
fn typed_ferrite_actions_come_under_the_shell_matches(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("copy", cx);
    assert_eq!(menu.rows(cx), ["Copy as path", "Copy", "Copy", "Copy path"]);
}

#[gpui_kit::test]
fn escape_clears_the_filter_then_dismisses(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("ext", cx);
    menu.press(&["escape"], cx);
    assert_eq!(menu.filter_text(cx), "");
    assert_eq!(menu.rows(cx), ROOT_ROWS);
    assert!(menu.events().is_empty());
    menu.press(&["escape"], cx);
    assert_eq!(menu.events(), [super::ContextMenuEvent::Dismissed]);
}

#[gpui_kit::test]
fn the_footer_counts_items_only_the_classic_menu_can_show(cx: &mut TestAppContext) {
    let menu = ShellMenu {
        items: vec![
            shell(1, "Open", Group::Open),
            shell(2, "", Group::Extensions),
            MenuItem::separator(Group::Other),
            shell(3, "", Group::Extensions),
        ],
    };
    let menu = open_menu_with(
        cx,
        gpui_kit::point(gpui_kit::px(40.), gpui_kit::px(20.)),
        menu,
    );
    assert_eq!(menu.rows(cx), ["Open"]);
    assert_eq!(
        menu.find(("menu-hint", 0), cx).as_deref(),
        Some("2 items only in the classic menu")
    );
}
