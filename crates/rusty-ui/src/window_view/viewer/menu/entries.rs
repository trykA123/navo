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
            MenuEntry::Run(label, action) => MenuItem {
                accelerator: hint(action.as_ref()),
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
        MenuEntry::Run("Mark 1", Box::new(Mark1)),
        MenuEntry::Run("Mark 2", Box::new(Mark2)),
        MenuEntry::Run("Mark 3", Box::new(Mark3)),
        MenuEntry::Run("Mark 4", Box::new(Mark4)),
        MenuEntry::Run("Mark 5", Box::new(Mark5)),
        MenuEntry::Run("Mark 6", Box::new(Mark6)),
        MenuEntry::Run("Mark 7", Box::new(Mark7)),
        MenuEntry::Run("Mark 8", Box::new(Mark8)),
        MenuEntry::Break,
        MenuEntry::Run("Clear marks", Box::new(ClearMarks)),
    ];
    vec![
        MenuEntry::Submenu("Mark colour", colours),
        MenuEntry::Run("Bookmark", Box::new(ToggleBookmark)),
    ]
}
