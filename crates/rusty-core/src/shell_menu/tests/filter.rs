use super::*;

fn hits(query: &str) -> Vec<ItemRow> {
    rows(&zip_menu(), query)
        .into_iter()
        .map(|row| match row {
            Row::Item(item) => item,
            other => panic!("filtered list holds {other:?}"),
        })
        .collect()
}

#[test]
fn query_reaches_filled_submenu_items_with_breadcrumb() {
    let breadcrumbs: Vec<String> = hits("extract").iter().map(ItemRow::breadcrumb).collect();
    assert_eq!(
        breadcrumbs,
        ["7-Zip › Extract Here", "7-Zip › Extract to \"dist\\\""]
    );
}

#[test]
fn match_is_case_insensitive_with_a_range() {
    let found = hits("HERE");
    assert_eq!(found.len(), 1);
    let range = found[0].matched.clone().unwrap();
    assert_eq!(&found[0].text[range], "Here");
}

#[test]
fn hidden_items_are_found_and_marked() {
    let found = hits("copy as");
    assert_eq!(found.len(), 1);
    assert!(found[0].hidden);
}

#[test]
fn children_of_hidden_submenus_are_marked_hidden() {
    let found = hits("sha-256");
    assert_eq!(found[0].breadcrumb(), "CRC SHA › SHA-256");
    assert!(found[0].hidden);
}

#[test]
fn ferrite_actions_come_first() {
    let texts: Vec<String> = hits("copy").into_iter().map(|row| row.text).collect();
    assert_eq!(texts, ["Copy path", "Copy name", "Copy as path", "Copy"]);
}

#[test]
fn disabled_items_follow_enabled_ones() {
    let menu = ShellMenu {
        items: vec![
            MenuItem {
                enabled: false,
                ..shell(1, "Print", Group::Open)
            },
            shell(2, "Print to PDF", Group::Open),
        ],
    };
    assert_eq!(item_texts(&rows(&menu, "print")), ["Print to PDF", "Print"]);
}

#[test]
fn rare_items_are_found() {
    assert_eq!(hits("shortcut")[0].text, "Create shortcut");
}

#[test]
fn matching_submenu_parent_still_opens() {
    let found = hits("send");
    assert_eq!(found.len(), 1);
    assert!(found[0].opens_submenu);
}

#[test]
fn lazy_submenus_contribute_no_children() {
    let menu = ShellMenu {
        items: vec![
            lazy(1, "Send to", Group::Share),
            MenuItem {
                kind: ItemKind::Submenu(Submenu::Loading),
                ..shell(2, "Open with", Group::Open)
            },
        ],
    };
    assert!(rows(&menu, "documents").is_empty());
}

#[test]
fn no_match_gives_no_rows() {
    assert!(hits("zzz").is_empty());
}

#[test]
fn match_range_survives_wider_lowercase() {
    let menu = ShellMenu {
        items: vec![shell(1, "Öffnen mit", Group::Open)],
    };
    let rows = rows(&menu, "öff");
    let Some(Row::Item(row)) = rows.first() else {
        panic!("no match");
    };
    assert_eq!(&row.text[row.matched.clone().unwrap()], "Öff");
}
