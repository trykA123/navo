use proptest::prelude::*;

use super::*;

fn group() -> impl Strategy<Value = Group> {
    prop_oneof![
        Just(Group::Ferrite),
        Just(Group::Open),
        Just(Group::Share),
        Just(Group::Edit),
        Just(Group::Extensions),
        Just(Group::Other),
    ]
}

fn leaf() -> impl Strategy<Value = MenuItem> {
    (0u32..64, "[a-cA-C &]{0,6}", group(), any::<[bool; 3]>()).prop_map(
        |(command, text, group, [enabled, hidden, separator])| {
            let item = MenuItem {
                enabled,
                hidden,
                ..shell(command, &text, group)
            };
            if separator {
                MenuItem::separator(group)
            } else {
                item
            }
        },
    )
}

fn item() -> impl Strategy<Value = MenuItem> {
    leaf().prop_recursive(2, 24, 6, |inner| {
        (leaf(), prop::collection::vec(inner, 0..6)).prop_map(|(parent, children)| MenuItem {
            kind: ItemKind::Submenu(Submenu::Filled(children)),
            ..parent
        })
    })
}

fn menu() -> impl Strategy<Value = ShellMenu> {
    prop::collection::vec(item(), 0..12).prop_map(|items| ShellMenu { items })
}

fn separators_are_clean(rows: &[Row]) -> bool {
    let items = match rows.first() {
        Some(Row::Back { .. }) => &rows[1..],
        _ => rows,
    };
    let first_ok = !matches!(items.first(), Some(Row::Separator));
    let last_ok = !matches!(items.last(), Some(Row::Separator));
    let no_pairs = items
        .windows(2)
        .all(|pair| !matches!(pair, [Row::Separator, Row::Separator]));
    first_ok && last_ok && no_pairs
}

proptest! {
    #[test]
    fn separators_never_lead_trail_or_double(menu in menu()) {
        prop_assert!(separators_are_clean(&rows(&menu, &[], "")));
        for index in 0..menu.items.len() {
            prop_assert!(separators_are_clean(&rows(&menu, &[index], "")));
        }
    }

    #[test]
    fn every_filtered_row_contains_the_query(menu in menu(), query in "[a-cA-C &]{1,3}") {
        let needle = query.trim().to_lowercase();
        prop_assume!(!needle.is_empty());
        for page in std::iter::once(Vec::new()).chain((0..menu.items.len()).map(|index| vec![index])) {
            for row in rows(&menu, &page, &query) {
                let item = match row {
                    Row::Item(item) => item,
                    Row::Back { .. } => continue,
                    Row::Separator => return Err(TestCaseError::fail("filtered list holds a separator")),
                };
                prop_assert!(item.path.starts_with(&page));
                prop_assert!(item.text.to_lowercase().contains(&needle));
                let range = item.matched.clone().unwrap_or_default();
                prop_assert_eq!(item.text[range].to_lowercase(), needle.clone());
            }
        }
    }

    #[test]
    fn selection_lands_only_on_selectable_rows(
        menu in menu(),
        query in prop_oneof![Just(String::new()), "[a-c]{1,2}"],
        moves in prop::collection::vec(any::<bool>(), 0..20),
    ) {
        let rows = rows(&menu, &[], &query);
        let any_enabled = rows.iter().any(|row| row.is_selectable(false));
        let mut selection = Selection::first(&rows);
        for forward in moves {
            selection = if forward { selection.next(&rows) } else { selection.previous(&rows) };
            if let Some(index) = selection.index() {
                prop_assert!(rows[index].is_selectable(!any_enabled));
            } else {
                prop_assert!(!rows.iter().any(|row| row.is_selectable(true)));
            }
        }
    }
}
