use gpui_kit::component::kbd::Kbd;
use gpui_kit::{Action, FocusHandle};
use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

use super::super::body::MenuEntry;
use super::super::{
    ClearMarks, Mark1, Mark2, Mark3, Mark4, Mark5, Mark6, Mark7, Mark8, ToggleBookmark,
};
use crate::prelude::*;

pub fn viewer_menu(entries: &[MenuEntry], focus: &FocusHandle, window: &Window) -> ShellMenu {
    ShellMenu {
        items: items(entries, &|action| hint(action, focus, window)),
    }
}

fn items(entries: &[MenuEntry], hint: &dyn Fn(&dyn Action) -> Option<String>) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Run(label, action, checked) => MenuItem {
                accelerator: hint(action.as_ref()),
                checked: *checked,
                ..MenuItem::new(ItemId::Action(action.name()), label, Group::Ferrite)
            },
            MenuEntry::Submenu(label, children) => MenuItem {
                kind: ItemKind::Submenu(Submenu::Filled(items(children, hint))),
                ..MenuItem::new(ItemId::Action(label), label, Group::Ferrite)
            },
            MenuEntry::Break => MenuItem::separator(Group::Ferrite),
        })
        .collect()
}

fn hint(action: &dyn Action, focus: &FocusHandle, window: &Window) -> Option<String> {
    let binding = window.highest_precedence_binding_for_action_in(action, focus)?;
    let keys: Vec<String> = binding
        .keystrokes()
        .iter()
        .map(|keystroke| Kbd::format(keystroke.inner()))
        .collect();
    Some(keys.join(" "))
}

pub(in crate::window_view::viewer) fn marking_entries() -> Vec<MenuEntry> {
    let colours: Vec<MenuEntry> = vec![
        MenuEntry::Run("Mark 1".into(), Box::new(Mark1), false),
        MenuEntry::Run("Mark 2".into(), Box::new(Mark2), false),
        MenuEntry::Run("Mark 3".into(), Box::new(Mark3), false),
        MenuEntry::Run("Mark 4".into(), Box::new(Mark4), false),
        MenuEntry::Run("Mark 5".into(), Box::new(Mark5), false),
        MenuEntry::Run("Mark 6".into(), Box::new(Mark6), false),
        MenuEntry::Run("Mark 7".into(), Box::new(Mark7), false),
        MenuEntry::Run("Mark 8".into(), Box::new(Mark8), false),
        MenuEntry::Break,
        MenuEntry::Run("Clear marks".into(), Box::new(ClearMarks), false),
    ];
    vec![
        MenuEntry::Submenu("Mark colour", colours),
        MenuEntry::Run("Bookmark".into(), Box::new(ToggleBookmark), false),
    ]
}
