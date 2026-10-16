use super::*;

#[test]
fn empty_query_shows_sections_groups_and_more() {
    assert_eq!(
        item_texts(&rows(&zip_menu(), "")),
        [
            "# Ferrite",
            "Copy path",
            "Copy name",
            "Rename",
            "Compare with other pane",
            "Open in other pane",
            "Properties",
            "# Windows",
            "Open",
            "Open with",
            "-",
            "7-Zip",
            "-",
            "Send to",
            "-",
            "Cut",
            "Copy",
            "-",
            "More (2)",
        ]
    );
}

#[test]
fn whitespace_query_counts_as_empty() {
    assert_eq!(rows(&zip_menu(), "  "), rows(&zip_menu(), ""));
}

#[test]
fn ferrite_only_menu_has_no_windows_header() {
    let menu = ShellMenu {
        items: ferrite_actions(ActionContext {
            count: 1,
            one_per_pane: false,
        }),
    };
    let rows = rows(&menu, "");
    assert!(!rows.contains(&Row::Header(Section::Windows)));
    assert_eq!(rows.len(), 7);
}

#[test]
fn rare_only_shell_items_show_more_without_separator() {
    let menu = ShellMenu {
        items: vec![MenuItem {
            rare: true,
            ..shell(1, "Pin", Group::Other)
        }],
    };
    assert_eq!(item_texts(&rows(&menu, "")), ["# Windows", "More (1)"]);
}

#[test]
fn leading_and_repeated_source_separators_collapse() {
    let menu = ShellMenu {
        items: vec![
            MenuItem::separator(Group::Open),
            shell(1, "Open", Group::Open),
            MenuItem::separator(Group::Open),
            MenuItem::separator(Group::Open),
            shell(2, "Print", Group::Open),
            MenuItem::separator(Group::Open),
        ],
    };
    assert_eq!(
        item_texts(&rows(&menu, "")),
        ["# Windows", "Open", "-", "Print"]
    );
}

#[test]
fn item_row_carries_its_path_and_submenu_flag() {
    let rows = rows(&zip_menu(), "");
    let Some(Row::Item(zip)) = rows
        .iter()
        .find(|row| matches!(row, Row::Item(item) if item.text == "7-Zip"))
    else {
        panic!("7-Zip row missing");
    };
    assert!(zip.opens_submenu);
    assert_eq!(zip_menu().item(&zip.path).unwrap().text, "7-Zip");
}

#[test]
fn submenu_rows_list_children_with_separators() {
    let menu = zip_menu();
    assert_eq!(
        item_texts(&submenu_rows(&menu, &[8])),
        ["Open archive", "Extract Here", "-", "Extract to \"dist\\\""]
    );
}

#[test]
fn submenu_rows_of_a_lazy_submenu_are_empty() {
    assert!(submenu_rows(&zip_menu(), &[7]).is_empty());
}

#[test]
fn rare_rows_list_the_folded_items() {
    assert_eq!(
        item_texts(&rare_rows(&zip_menu())),
        ["Create shortcut", "Restore previous versions"]
    );
}

#[test]
fn item_lookup_follows_nested_paths() {
    let menu = zip_menu();
    assert_eq!(menu.item(&[8, 1]).unwrap().text, "Extract Here");
    assert!(menu.item(&[8, 9]).is_none());
    assert!(menu.item(&[]).is_none());
}
