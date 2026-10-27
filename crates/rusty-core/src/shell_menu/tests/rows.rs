use super::*;

fn root(menu: &ShellMenu) -> Vec<String> {
    item_texts(&rows(menu, &[], ""))
}

#[test]
fn the_root_lists_the_shell_items_in_order_with_their_separators() {
    assert_eq!(
        root(&zip_menu()),
        [
            "Open",
            "Open with",
            "7-Zip",
            "-",
            "Send to",
            "-",
            "Cut",
            "Copy",
            "-",
            "Create shortcut",
            "Restore previous versions",
        ]
    );
}

#[test]
fn whitespace_query_counts_as_empty() {
    assert_eq!(rows(&zip_menu(), &[], "  "), rows(&zip_menu(), &[], ""));
}

#[test]
fn groups_alone_never_add_a_separator() {
    let menu = ShellMenu {
        items: vec![
            shell(1, "Open", Group::Open),
            shell(2, "Cut", Group::Edit),
            shell(3, "Pin", Group::Other),
        ],
    };
    assert_eq!(root(&menu), ["Open", "Cut", "Pin"]);
}

#[test]
fn visible_ferrite_items_are_listed_where_they_stand() {
    let action = |name: &'static str| MenuItem::new(ItemId::Action(name), name, Group::Ferrite);
    let menu = ShellMenu {
        items: vec![
            MenuItem::separator(Group::Ferrite),
            action("Copy"),
            action("Select all"),
            MenuItem::separator(Group::Ferrite),
            action("Bookmark"),
        ],
    };
    assert_eq!(root(&menu), ["Copy", "Select all", "-", "Bookmark"]);
}

#[test]
fn hidden_ferrite_actions_stay_out_of_the_list() {
    let rename = ItemId::Ferrite(FerriteAction::Rename);
    let rows = rows(&zip_menu(), &[], "");
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, Row::Item(item) if item.id == rename))
    );
}

#[test]
fn leading_trailing_and_repeated_separators_collapse() {
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
    assert_eq!(root(&menu), ["Open", "-", "Print"]);
}

#[test]
fn item_row_carries_its_path_and_submenu_flag() {
    let rows = rows(&zip_menu(), &[], "");
    let Some(Row::Item(zip)) = rows
        .iter()
        .find(|row| matches!(row, Row::Item(item) if item.text == "7-Zip"))
    else {
        panic!("7-Zip row missing");
    };
    assert!(zip.opens_submenu);
    assert_eq!(zip.path, [SEVEN_ZIP]);
}

#[test]
fn a_page_lists_its_children() {
    assert_eq!(
        item_texts(&rows(&zip_menu(), &[SEVEN_ZIP], "")),
        ["Open archive", "Extract Here", "-", "Extract to \"dist\\\""]
    );
}

#[test]
fn page_rows_carry_full_paths() {
    let rows = rows(&zip_menu(), &[SEVEN_ZIP], "");
    let Some(Row::Item(here)) = rows.get(1) else {
        panic!("Extract Here missing");
    };
    assert_eq!(here.path, [SEVEN_ZIP, 1]);
}

#[test]
fn a_lazy_page_has_no_rows() {
    assert!(rows(&zip_menu(), &[SEND_TO], "").is_empty());
}

#[test]
fn a_page_that_does_not_exist_has_no_rows() {
    assert!(rows(&zip_menu(), &[99], "").is_empty());
    assert!(rows(&zip_menu(), &[SEVEN_ZIP, 9], "").is_empty());
}

#[test]
fn item_lookup_follows_nested_paths() {
    let menu = zip_menu();
    assert_eq!(menu.item(&[SEVEN_ZIP, 1]).unwrap().text, "Extract Here");
    assert!(menu.item(&[SEVEN_ZIP, 9]).is_none());
    assert!(menu.item(&[]).is_none());
}

#[test]
fn items_without_text_are_skipped_and_counted_for_the_classic_menu() {
    let menu = ShellMenu {
        items: vec![
            shell(1, "Open", Group::Open),
            shell(2, "  ", Group::Open),
            submenu(3, "Tools", Group::Other, vec![shell(30, "", Group::Other)]),
            MenuItem::separator(Group::Other),
        ],
    };
    assert_eq!(root(&menu), ["Open", "Tools"]);
    assert!(rows(&menu, &[2], "").is_empty());
    assert_eq!(menu.classic_only_count(), 2);
}

#[test]
fn filled_lazy_submenu_shows_its_children() {
    let mut menu = zip_menu();
    assert!(menu.set_submenu(
        &[SEND_TO],
        Submenu::Filled(vec![shell(50, "Documents", Group::Share)])
    ));
    assert_eq!(
        item_texts(&rows(&menu, &[], "docu")),
        ["Send to › Documents"]
    );
    assert_eq!(item_texts(&rows(&menu, &[SEND_TO], "")), ["Documents"]);
}

#[test]
fn set_submenu_refuses_commands_and_bad_paths() {
    let mut menu = zip_menu();
    assert!(!menu.set_submenu(&[0], Submenu::Loading));
    assert!(!menu.set_submenu(&[129], Submenu::Loading));
    assert!(menu.set_submenu(&[SEVEN_ZIP], Submenu::Failed));
    assert!(rows(&menu, &[SEVEN_ZIP], "").is_empty());
}

#[test]
fn a_tree_of_separators_and_unnamed_items_has_no_named_item() {
    let unnamed = ShellMenu {
        items: vec![
            MenuItem::separator(Group::Open),
            shell(1, "&", Group::Open),
            shell(2, "", Group::Other),
        ],
    };
    assert!(!unnamed.has_named_item());
    assert!(!ShellMenu::default().has_named_item());
    assert!(zip_menu().has_named_item());
}
