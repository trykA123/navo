use gpui_kit::component::kbd::Kbd;
use gpui_kit::{Action, FocusHandle};
use navo_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

use crate::prelude::*;

pub enum MenuEntry {
    Run(SharedString, Box<dyn Action>, bool),
    Submenu(&'static str, Vec<MenuEntry>),
    Break,
}

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
                ..MenuItem::new(ItemId::Action(action.name()), label, Group::Navo)
            },
            MenuEntry::Submenu(label, children) => MenuItem {
                kind: ItemKind::Submenu(Submenu::Filled(items(children, hint))),
                ..MenuItem::new(ItemId::Action(label), label, Group::Navo)
            },
            MenuEntry::Break => MenuItem::separator(Group::Navo),
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
