use navo_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, NavoAction, ShellMenu, Submenu};

use super::merge::{self, Own};

use crate::commands::Keymap;
use crate::file_list::{SortBy, SortKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundLook {
    pub sort: SortKey,
    pub can_paste: bool,
    pub has_properties: bool,
    /// A compare with the other pane can start from here.
    pub can_compare: bool,
}

const SORTS: [(SortBy, &str, &str, &str, char); 4] = [
    (
        SortBy::Name,
        "file_list::SortName",
        "Name",
        "files.sort_by_name",
        'N',
    ),
    (
        SortBy::Ext,
        "file_list::SortExt",
        "Extension",
        "files.sort_by_extension",
        'E',
    ),
    (
        SortBy::Date,
        "file_list::SortDate",
        "Date",
        "files.sort_by_date",
        'D',
    ),
    (
        SortBy::Size,
        "file_list::SortSize",
        "Size",
        "files.sort_by_size",
        'S',
    ),
];

fn navo(action: NavoAction, raw: &str, key: char) -> MenuItem {
    MenuItem {
        access_key: Some(key),
        ..MenuItem::new(ItemId::Navo(action), raw, Group::Navo)
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

fn sort_menu(current: SortKey, keys: &Keymap) -> MenuItem {
    let children = SORTS
        .into_iter()
        .map(|(by, action, name, command, key)| {
            let shown = (by == current.by).then_some(current);
            let label = sort_label(name, shown);
            let raw = match keys.shown_in(command, "FileList") {
                Some(key) => format!("{label}\t{key}"),
                None => label,
            };
            MenuItem {
                access_key: Some(key),
                checked: shown.is_some(),
                ..MenuItem::new(ItemId::Action(action), &raw, Group::Navo)
            }
        })
        .collect();
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..navo(NavoAction::SortBy, "Sort by", 'S')
    }
}

fn new_folder() -> MenuItem {
    navo(NavoAction::NewFolder, "New folder\tF7", 'N')
}

fn new_file() -> MenuItem {
    navo(NavoAction::NewFile, "New file…\tShift+F7", 'F')
}

fn paste(look: &BackgroundLook) -> MenuItem {
    MenuItem {
        enabled: look.can_paste,
        ..navo(NavoAction::Paste, "Paste\tCtrl+V", 'P')
    }
}

fn refresh() -> MenuItem {
    navo(NavoAction::Refresh, "Refresh\tCtrl+R", 'R')
}

fn open_terminal() -> MenuItem {
    MenuItem::new(
        ItemId::Action("navo_window::OpenTerminal"),
        &navo_l10n::tr("cmd-files-open-terminal"),
        Group::Navo,
    )
}

fn compare_with_other_pane() -> MenuItem {
    MenuItem::new(
        ItemId::Action("compare::WithOtherPane"),
        &navo_l10n::tr("cmd-compare-with-other-pane"),
        Group::Navo,
    )
}

fn copy_folder_path() -> MenuItem {
    navo(NavoAction::CopyPath, "Copy folder path", 'C')
}

/// With a shell list, Sort by and Refresh lead it and New file follows New ▸, as Explorer places them.
pub(super) fn background_own(look: &BackgroundLook, has_shell: bool, keys: &Keymap) -> Own {
    if has_shell {
        let mut top = vec![
            sort_menu(look.sort, keys),
            refresh(),
            copy_folder_path(),
            open_terminal(),
        ];
        top.extend(look.can_compare.then(compare_with_other_pane));
        return Own {
            top,
            after_new: vec![new_file()],
            hidden: vec![new_folder(), paste(look)],
        };
    }
    let mut top = vec![
        new_folder(),
        new_file(),
        paste(look),
        refresh(),
        sort_menu(look.sort, keys),
        copy_folder_path(),
        open_terminal(),
    ];
    top.extend(look.can_compare.then(compare_with_other_pane));
    if look.has_properties {
        top.push(navo(NavoAction::Properties, "Properties", 'I'));
    }
    Own {
        top,
        ..Own::default()
    }
}

/// The folder menu on Windows: Navo's items placed into the shell list.
#[must_use]
pub fn merged_background_menu(shell: &[MenuItem], look: &BackgroundLook) -> ShellMenu {
    merge::merge(shell, &background_own(look, true, Keymap::builtin())).menu
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(can_compare: bool) -> BackgroundLook {
        BackgroundLook {
            sort: SortKey::default(),
            can_paste: false,
            has_properties: false,
            can_compare,
        }
    }

    fn offers_compare(own: &Own) -> bool {
        let id = ItemId::Action("compare::WithOtherPane");
        own.top.iter().any(|item| item.id == id)
    }

    #[test]
    fn the_folder_menu_offers_compare_only_when_one_can_start() {
        for has_shell in [false, true] {
            assert!(offers_compare(&background_own(
                &look(true),
                has_shell,
                Keymap::builtin()
            )));
            assert!(!offers_compare(&background_own(
                &look(false),
                has_shell,
                Keymap::builtin()
            )));
        }
    }

    #[test]
    fn the_sort_items_show_the_keys_of_the_keymap() {
        let text = "[[bind]]\ncommand = \"files.sort_by_date\"\nkeys = [\"ctrl-alt-d\"]\n";
        let keys = Keymap::builtin().overlay(text).expect("keymap").keymap;
        let accelerator = |keys: &Keymap, at: usize| match sort_menu(SortKey::default(), keys).kind
        {
            ItemKind::Submenu(Submenu::Filled(items)) => items[at].accelerator.clone(),
            _ => None,
        };
        assert_eq!(accelerator(&keys, 2).as_deref(), Some("Ctrl+Alt+D"));
        assert_eq!(
            accelerator(Keymap::builtin(), 0).as_deref(),
            Some("Ctrl+F3")
        );
    }
}
