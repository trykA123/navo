use gpui_kit::TestAppContext;

use super::open_default;

#[gpui_kit::test]
fn typing_narrows_the_rows_to_matches_and_highlights_them(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    assert_eq!(menu.rows(0, cx).len(), 24);
    menu.type_text("ext", cx);
    assert_eq!(
        menu.rows(0, cx),
        [
            "7-Zip › Extract files…",
            "7-Zip › Extract Here",
            "7-Zip › Extract to \"dist\\\"",
        ]
    );
    assert_eq!(menu.find(("menu-match", 0), cx).as_deref(), Some("Ext"));
    assert_eq!(menu.find(("menu-match", 1), cx).as_deref(), Some("Ext"));
    assert_eq!(
        menu.selected(0, cx).as_deref(),
        Some("7-Zip › Extract files…")
    );
}

#[gpui_kit::test]
fn the_overview_has_section_headers_and_the_more_fold(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    let rows = menu.rows(0, cx);
    assert_eq!(rows[0], "# Ferrite");
    assert_eq!(rows[11], "# Windows");
    assert_eq!(rows[14], "-");
    assert_eq!(rows[23], "More (2)");
    assert!(!rows.iter().any(|row| row == "Copy as path"));
    assert_eq!(
        menu.find(("menu-hint", 0), cx).as_deref(),
        Some("1 hidden, 2 rare: type to reach")
    );
}

#[gpui_kit::test]
fn enter_on_more_unfolds_the_rare_commands(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["up", "enter"], cx);
    let rows = menu.rows(0, cx);
    assert_eq!(
        &rows[23..],
        ["Create shortcut", "Restore previous versions"]
    );
    assert_eq!(menu.selected(0, cx).as_deref(), Some("Create shortcut"));
    assert!(menu.events().is_empty());
}

#[gpui_kit::test]
fn escape_clears_the_filter_then_dismisses(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.type_text("ext", cx);
    menu.press(&["escape"], cx);
    assert_eq!(menu.filter_text(cx), "");
    assert_eq!(menu.rows(0, cx).len(), 24);
    assert!(menu.events().is_empty());
    menu.press(&["escape"], cx);
    assert_eq!(menu.events(), [super::ContextMenuEvent::Dismissed]);
}
