use super::{Group, ItemId, MenuItem, NavoAction};

const ACTIONS: [(NavoAction, &str, char); 10] = [
    (NavoAction::Open, "Open\tEnter", 'O'),
    (NavoAction::View, "View\tF3", 'V'),
    (NavoAction::Edit, "Edit\tF4", 'E'),
    (NavoAction::Copy, "Copy\tF5", 'C'),
    (NavoAction::Move, "Move\tF6", 'M'),
    (NavoAction::Rename, "Rename\tF2", 'R'),
    (NavoAction::Delete, "Delete\tF8", 'D'),
    (NavoAction::CopyPath, "Copy path", 'P'),
    (NavoAction::Properties, "Properties", 'I'),
    (NavoAction::ClassicMenu, "Classic menu\tShift+F10", 'L'),
];

#[must_use]
pub fn navo_actions() -> Vec<MenuItem> {
    ACTIONS
        .into_iter()
        .map(|(action, text, key)| MenuItem {
            access_key: Some(key),
            ..MenuItem::new(ItemId::Navo(action), text, Group::Navo)
        })
        .collect()
}
