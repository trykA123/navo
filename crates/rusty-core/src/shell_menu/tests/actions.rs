use super::*;

fn action(items: &[MenuItem], action: FerriteAction) -> &MenuItem {
    items
        .iter()
        .find(|item| item.id == ItemId::Ferrite(action))
        .unwrap()
}

#[test]
fn lists_the_file_actions_in_order() {
    let ids: Vec<ItemId> = ferrite_actions().into_iter().map(|item| item.id).collect();
    assert_eq!(
        ids,
        [
            FerriteAction::Open,
            FerriteAction::View,
            FerriteAction::Edit,
            FerriteAction::Copy,
            FerriteAction::Move,
            FerriteAction::Rename,
            FerriteAction::Delete,
            FerriteAction::CopyPath,
            FerriteAction::Properties,
            FerriteAction::ClassicMenu,
        ]
        .map(ItemId::Ferrite)
    );
}

#[test]
fn actions_sit_in_the_ferrite_group() {
    assert!(ferrite_actions().iter().all(MenuItem::is_ferrite));
}

#[test]
fn actions_show_their_function_keys() {
    let items = ferrite_actions();
    let keys = [
        (FerriteAction::Open, "Enter"),
        (FerriteAction::View, "F3"),
        (FerriteAction::Edit, "F4"),
        (FerriteAction::Copy, "F5"),
        (FerriteAction::Move, "F6"),
        (FerriteAction::Rename, "F2"),
        (FerriteAction::Delete, "F8"),
    ];
    for (id, key) in keys {
        assert_eq!(
            action(&items, id).accelerator.as_deref(),
            Some(key),
            "{id:?}"
        );
    }
}

#[test]
fn rename_keeps_its_plain_text() {
    let items = ferrite_actions();
    assert_eq!(action(&items, FerriteAction::Rename).text, "Rename");
}

#[test]
fn every_action_is_enabled() {
    assert!(ferrite_actions().iter().all(|item| item.enabled));
}

#[test]
fn access_keys_are_unique() {
    let mut keys: Vec<char> = ferrite_actions()
        .into_iter()
        .filter_map(|item| item.access_key)
        .collect();
    let count = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), count);
    assert_eq!(count, 10);
}

#[test]
fn classic_menu_comes_last_with_its_key() {
    let items = ferrite_actions();
    let classic = items.last().unwrap();
    assert_eq!(classic.id, ItemId::Ferrite(FerriteAction::ClassicMenu));
    assert_eq!(classic.text, "Classic menu");
    assert_eq!(classic.accelerator.as_deref(), Some("Shift+F10"));
}
