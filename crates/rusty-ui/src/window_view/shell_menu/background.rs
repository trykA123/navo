use rusty_core::shell_menu::{FerriteAction, Group, ItemId, ItemKind, MenuItem, Submenu};

use crate::file_list::{SortBy, SortKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundLook {
    pub sort: SortKey,
    pub can_paste: bool,
    pub has_properties: bool,
}

const SORTS: [(SortBy, &str, &str, char); 4] = [
    (SortBy::Name, "file_list::SortName", "Name\tCtrl+F3", 'N'),
    (SortBy::Ext, "file_list::SortExt", "Extension\tCtrl+F4", 'E'),
    (SortBy::Date, "file_list::SortDate", "Date\tCtrl+F5", 'D'),
    (SortBy::Size, "file_list::SortSize", "Size\tCtrl+F6", 'S'),
];

fn ferrite(action: FerriteAction, raw: &str, key: char) -> MenuItem {
    MenuItem {
        access_key: Some(key),
        ..MenuItem::new(ItemId::Ferrite(action), raw, Group::Ferrite)
    }
}

fn sort_menu(current: SortBy) -> MenuItem {
    let children = SORTS
        .into_iter()
        .map(|(by, action, raw, key)| MenuItem {
            access_key: Some(key),
            checked: by == current,
            ..MenuItem::new(ItemId::Action(action), raw, Group::Ferrite)
        })
        .collect();
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..ferrite(FerriteAction::SortBy, "Sort by", 'S')
    }
}

#[must_use]
pub fn background_items(look: &BackgroundLook) -> Vec<MenuItem> {
    let mut items = vec![
        ferrite(FerriteAction::NewFolder, "New folder\tF7", 'N'),
        ferrite(FerriteAction::NewFile, "New file…\tShift+F7", 'F'),
        MenuItem {
            enabled: look.can_paste,
            ..ferrite(FerriteAction::Paste, "Paste\tCtrl+V", 'P')
        },
        ferrite(FerriteAction::Refresh, "Refresh\tCtrl+R", 'R'),
        sort_menu(look.sort.by),
        ferrite(FerriteAction::CopyPath, "Copy folder path", 'C'),
    ];
    if look.has_properties {
        items.push(ferrite(FerriteAction::Properties, "Properties", 'I'));
    }
    items
}
