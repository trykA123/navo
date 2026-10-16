use super::*;

fn context(count: usize, one_per_pane: bool) -> ActionContext {
    ActionContext {
        count,
        one_per_pane,
    }
}

fn action(items: &[MenuItem], action: FerriteAction) -> &MenuItem {
    items
        .iter()
        .find(|item| item.id == ItemId::Ferrite(action))
        .unwrap()
}

#[test]
fn single_selection_lists_six_actions_in_order() {
    let ids: Vec<ItemId> = ferrite_actions(context(1, false))
        .into_iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(
        ids,
        [
            FerriteAction::CopyPath,
            FerriteAction::CopyName,
            FerriteAction::Rename,
            FerriteAction::Compare,
            FerriteAction::OpenInOtherPane,
            FerriteAction::Properties,
        ]
        .map(ItemId::Ferrite)
    );
}

#[test]
fn actions_sit_in_the_ferrite_group() {
    assert!(
        ferrite_actions(context(1, false))
            .iter()
            .all(MenuItem::is_ferrite)
    );
}

#[test]
fn single_rename_keeps_its_key() {
    let items = ferrite_actions(context(1, false));
    let rename = action(&items, FerriteAction::Rename);
    assert_eq!(rename.text, "Rename");
    assert_eq!(rename.accelerator.as_deref(), Some("F2"));
}

#[test]
fn multi_selection_turns_rename_into_multi_rename() {
    let items = ferrite_actions(context(3, false));
    assert_eq!(
        action(&items, FerriteAction::Rename).text,
        "Multi-rename 3 files…"
    );
}

#[test]
fn compare_is_disabled_for_one_item() {
    let items = ferrite_actions(context(1, false));
    assert!(!action(&items, FerriteAction::Compare).enabled);
}

#[test]
fn compare_is_enabled_for_exactly_two() {
    let items = ferrite_actions(context(2, false));
    assert!(action(&items, FerriteAction::Compare).enabled);
}

#[test]
fn compare_is_disabled_for_three() {
    let items = ferrite_actions(context(3, false));
    assert!(!action(&items, FerriteAction::Compare).enabled);
}

#[test]
fn compare_is_enabled_with_one_item_per_pane() {
    let items = ferrite_actions(context(1, true));
    assert!(action(&items, FerriteAction::Compare).enabled);
}

#[test]
fn actions_carry_access_keys() {
    let keys: Vec<Option<char>> = ferrite_actions(context(1, false))
        .into_iter()
        .map(|item| item.access_key)
        .collect();
    assert_eq!(keys, ['P', 'N', 'R', 'M', 'O', 'I'].map(Some));
}
