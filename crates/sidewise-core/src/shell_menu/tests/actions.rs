use super::*;

fn action(items: &[MenuItem], action: NavoAction) -> &MenuItem {
    items
        .iter()
        .find(|item| item.id == ItemId::Navo(action))
        .unwrap()
}

#[test]
fn lists_the_file_actions_in_order() {
    let ids: Vec<ItemId> = navo_actions().into_iter().map(|item| item.id).collect();
    assert_eq!(
        ids,
        [
            NavoAction::Open,
            NavoAction::View,
            NavoAction::Edit,
            NavoAction::Copy,
            NavoAction::Move,
            NavoAction::Rename,
            NavoAction::Delete,
            NavoAction::CopyPath,
            NavoAction::Properties,
            NavoAction::ClassicMenu,
        ]
        .map(ItemId::Navo)
    );
}

#[test]
fn actions_sit_in_the_navo_group() {
    assert!(navo_actions().iter().all(MenuItem::is_navo));
}

#[test]
fn actions_show_their_function_keys() {
    let items = navo_actions();
    let keys = [
        (NavoAction::Open, "Enter"),
        (NavoAction::View, "F3"),
        (NavoAction::Edit, "F4"),
        (NavoAction::Copy, "F5"),
        (NavoAction::Move, "F6"),
        (NavoAction::Rename, "F2"),
        (NavoAction::Delete, "F8"),
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
    let items = navo_actions();
    assert_eq!(action(&items, NavoAction::Rename).text, "Rename");
}

#[test]
fn every_action_is_enabled() {
    assert!(navo_actions().iter().all(|item| item.enabled));
}

#[test]
fn access_keys_are_unique() {
    let mut keys: Vec<char> = navo_actions()
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
    let items = navo_actions();
    let classic = items.last().unwrap();
    assert_eq!(classic.id, ItemId::Navo(NavoAction::ClassicMenu));
    assert_eq!(classic.text, "Classic menu");
    assert_eq!(classic.accelerator.as_deref(), Some("Shift+F10"));
}
