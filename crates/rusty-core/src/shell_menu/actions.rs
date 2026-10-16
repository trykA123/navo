use super::{FerriteAction, Group, ItemId, MenuItem};

const ACTIONS: [(FerriteAction, &str, char); 10] = [
    (FerriteAction::Open, "Open\tEnter", 'O'),
    (FerriteAction::View, "View\tF3", 'V'),
    (FerriteAction::Edit, "Edit\tF4", 'E'),
    (FerriteAction::Copy, "Copy\tF5", 'C'),
    (FerriteAction::Move, "Move\tF6", 'M'),
    (FerriteAction::Rename, "Rename\tF2", 'R'),
    (FerriteAction::Delete, "Delete\tF8", 'D'),
    (FerriteAction::CopyPath, "Copy path", 'P'),
    (FerriteAction::Properties, "Properties", 'I'),
    (FerriteAction::ClassicMenu, "Classic menu\tShift+F10", 'L'),
];

#[must_use]
pub fn ferrite_actions() -> Vec<MenuItem> {
    ACTIONS
        .into_iter()
        .map(|(action, text, key)| MenuItem {
            access_key: Some(key),
            ..MenuItem::new(ItemId::Ferrite(action), text, Group::Ferrite)
        })
        .collect()
}
