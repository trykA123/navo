use rusty_core::shell_menu::{
    FerriteAction, Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu,
};

use super::merge::{self, Own};

use crate::file_list::{SortBy, SortKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundLook {
    pub sort: SortKey,
    pub can_paste: bool,
    pub has_properties: bool,
}

const SORTS: [(SortBy, &str, &str, &str, char); 4] = [
    (SortBy::Name, "file_list::SortName", "Name", "Ctrl+F3", 'N'),
    (
        SortBy::Ext,
        "file_list::SortExt",
        "Extension",
        "Ctrl+F4",
        'E',
    ),
    (SortBy::Date, "file_list::SortDate", "Date", "Ctrl+F5", 'D'),
    (SortBy::Size, "file_list::SortSize", "Size", "Ctrl+F6", 'S'),
];

fn ferrite(action: FerriteAction, raw: &str, key: char) -> MenuItem {
    MenuItem {
        access_key: Some(key),
        ..MenuItem::new(ItemId::Ferrite(action), raw, Group::Ferrite)
    }
}

fn sort_label(name: &str, current: Option<SortKey>) -> String {
    match current {
        Some(SortKey {
            descending: false, ..
        }) => format!("{name} ↑"),
        Some(SortKey {
            descending: true, ..
        }) => format!("{name} ↓"),
        None => name.to_owned(),
    }
}

fn sort_menu(current: SortKey) -> MenuItem {
    let children = SORTS
        .into_iter()
        .map(|(by, action, name, keys, key)| {
            let shown = (by == current.by).then_some(current);
            let raw = format!("{}\t{keys}", sort_label(name, shown));
            MenuItem {
                access_key: Some(key),
                checked: shown.is_some(),
                ..MenuItem::new(ItemId::Action(action), &raw, Group::Ferrite)
            }
        })
        .collect();
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..ferrite(FerriteAction::SortBy, "Sort by", 'S')
    }
}

fn new_folder() -> MenuItem {
    ferrite(FerriteAction::NewFolder, "New folder\tF7", 'N')
}

fn new_file() -> MenuItem {
    ferrite(FerriteAction::NewFile, "New file…\tShift+F7", 'F')
}

fn paste(look: &BackgroundLook) -> MenuItem {
    MenuItem {
        enabled: look.can_paste,
        ..ferrite(FerriteAction::Paste, "Paste\tCtrl+V", 'P')
    }
}

fn refresh() -> MenuItem {
    ferrite(FerriteAction::Refresh, "Refresh\tCtrl+R", 'R')
}

fn copy_folder_path() -> MenuItem {
    ferrite(FerriteAction::CopyPath, "Copy folder path", 'C')
}

/// With a shell list, Sort by and Refresh lead it and New file follows New ▸, as Explorer places them.
pub(super) fn background_own(look: &BackgroundLook, has_shell: bool) -> Own {
    if has_shell {
        return Own {
            top: vec![sort_menu(look.sort), refresh()],
            after_new: vec![new_file()],
            hidden: vec![new_folder(), paste(look), copy_folder_path()],
        };
    }
    let mut top = vec![
        new_folder(),
        new_file(),
        paste(look),
        refresh(),
        sort_menu(look.sort),
        copy_folder_path(),
    ];
    if look.has_properties {
        top.push(ferrite(FerriteAction::Properties, "Properties", 'I'));
    }
    Own {
        top,
        ..Own::default()
    }
}

/// The folder menu on Windows: Navo's items placed into the shell list.
#[must_use]
pub fn merged_background_menu(shell: &[MenuItem], look: &BackgroundLook) -> ShellMenu {
    merge::merge(shell, &background_own(look, true)).menu
}
