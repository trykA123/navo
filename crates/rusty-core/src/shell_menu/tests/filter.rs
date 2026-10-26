use super::*;

fn hits(query: &str) -> Vec<ItemRow> {
    hits_in(&[], query)
}

fn hits_in(page: &[usize], query: &str) -> Vec<ItemRow> {
    rows(&zip_menu(), page, query)
        .into_iter()
        .filter_map(|row| match row {
            Row::Item(item) => Some(item),
            Row::Back { .. } => None,
            Row::Separator => panic!("a filtered list holds a separator"),
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
fn ferrite_actions_follow_the_shell_matches() {
    let texts: Vec<String> = hits("copy").into_iter().map(|row| row.text).collect();
    assert_eq!(texts, ["Copy as path", "Copy", "Copy", "Copy path"]);
    let copy = &hits("copy")[2];
    assert_eq!(copy.id, ItemId::Ferrite(FerriteAction::Copy));
}

#[test]
fn hidden_ferrite_actions_are_found_by_typing() {
    let found = hits("rename");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, ItemId::Ferrite(FerriteAction::Rename));
}

#[test]
fn a_query_inside_a_page_finds_only_its_descendants() {
    let found: Vec<String> = hits_in(&[SEVEN_ZIP], "o")
        .iter()
        .map(ItemRow::breadcrumb)
        .collect();
    assert_eq!(found, ["Open archive", "Extract to \"dist\\\""]);
    assert!(hits_in(&[SEVEN_ZIP], "copy").is_empty());
}

#[test]
fn a_page_query_keeps_the_back_row_and_full_paths() {
    let rows = rows(&zip_menu(), &[SEVEN_ZIP], "here");
    assert_eq!(item_texts(&rows), ["< 7-Zip", "Extract Here"]);
    let Some(Row::Item(here)) = rows.get(1) else {
        panic!("Extract Here missing");
    };
    assert_eq!(here.path, [SEVEN_ZIP, 1]);
}

#[test]
fn crumbs_inside_a_page_start_below_it() {
    let mut menu = zip_menu();
    let deep = submenu(
        33,
        "More",
        Group::Extensions,
        vec![shell(34, "Benchmark", Group::Extensions)],
    );
    let Some(ItemKind::Submenu(Submenu::Filled(children))) =
        menu.item_mut(&[SEVEN_ZIP]).map(|item| &mut item.kind)
    else {
        panic!("7-Zip is filled");
    };
    children.push(deep);
    let rows = rows(&menu, &[SEVEN_ZIP], "bench");
    assert_eq!(item_texts(&rows), ["< 7-Zip", "More › Benchmark"]);
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
    assert_eq!(
        item_texts(&rows(&menu, &[], "print")),
        ["Print to PDF", "Print"]
    );
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
    assert!(rows(&menu, &[], "documents").is_empty());
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
    let rows = rows(&menu, &[], "öff");
    let Some(Row::Item(row)) = rows.first() else {
        panic!("no match");
    };
    assert_eq!(&row.text[row.matched.clone().unwrap()], "Öff");
}

#[test]
fn children_of_disabled_submenus_are_disabled() {
    let menu = ShellMenu {
        items: vec![MenuItem {
            enabled: false,
            ..submenu(
                1,
                "Git",
                Group::Extensions,
                vec![shell(10, "Pull", Group::Extensions)],
            )
        }],
    };
    let rows = rows(&menu, &[], "pull");
    let Some(Row::Item(pull)) = rows.first() else {
        panic!("Pull not found");
    };
    assert!(!pull.enabled);
    assert_eq!(Selection::first(&rows).enter(&rows), None);
}
