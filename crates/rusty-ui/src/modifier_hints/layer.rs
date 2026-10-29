use gpui_kit::{Keystroke, SharedString};
use rusty_l10n::tr;

use super::combo::Combo;
use super::groups::{ORDER, place_of};
use super::runs::folded;
use crate::commands::{Bind, CommandSpec, GLOBAL, Keymap};

const CONTEXTS: [&str; 3] = [GLOBAL, "NavoPane", "FileList"];

const CAPS: [(&str, &str); 14] = [
    ("pagedown", "PgDn"),
    ("pageup", "PgUp"),
    ("delete", "Del"),
    ("insert", "Ins"),
    ("escape", "Esc"),
    ("backspace", "Bksp"),
    ("left", "←"),
    ("right", "→"),
    ("up", "↑"),
    ("down", "↓"),
    ("enter", "Enter"),
    ("space", "Space"),
    ("home", "Home"),
    ("end", "End"),
];

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    pub command: &'static str,
    pub label: SharedString,
    pub keys: Vec<SharedString>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Group {
    pub name: &'static str,
    pub entries: Vec<Entry>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layer {
    pub combo: Combo,
    pub groups: Vec<Group>,
    pub more: Vec<Entry>,
}

impl Layer {
    #[must_use]
    pub fn count(&self) -> usize {
        self.groups.iter().map(|group| group.entries.len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    #[must_use]
    pub fn rows(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.entries.len())
            .chain([self.more.len()])
            .max()
            .unwrap_or(0)
    }
}

#[must_use]
pub fn key_cap(key: &str) -> String {
    if let Some((_, cap)) = CAPS.iter().find(|(name, _)| *name == key) {
        return (*cap).to_owned();
    }
    let mut chars = key.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn caps_on(bind: &Bind, combo: Combo) -> Vec<SharedString> {
    bind.keys
        .iter()
        .filter_map(|key| Keystroke::parse(key).ok())
        .filter(|stroke| combo.matches(&stroke.modifiers))
        .map(|stroke| key_cap(&stroke.key).into())
        .collect()
}

pub type IsHandled<'a> = &'a dyn Fn(&CommandSpec) -> bool;

fn entries(keymap: &Keymap, combo: Combo, is_handled: IsHandled) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let binds = keymap
        .binds()
        .iter()
        .filter(|bind| CONTEXTS.contains(&bind.context.as_str()) && is_handled(&bind.spec));
    for bind in binds {
        let caps = caps_on(bind, combo);
        if caps.is_empty() {
            continue;
        }
        let name = bind.spec.name;
        let Some(entry) = entries.iter_mut().find(|entry| entry.command == name) else {
            entries.push(Entry {
                command: name,
                label: tr(bind.spec.label).into(),
                keys: caps,
            });
            continue;
        };
        entry.keys.extend(caps);
    }
    for entry in &mut entries {
        let mut seen = Vec::new();
        entry.keys.retain(|key| {
            let is_new = !seen.contains(key);
            seen.push(key.clone());
            is_new
        });
    }
    entries.sort_by_key(|entry| place_of(entry.command));
    folded(&entries)
}

fn grouped(entries: Vec<Entry>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for entry in entries {
        let name = ORDER[place_of(entry.command).0];
        match groups.last_mut().filter(|group| group.name == name) {
            Some(group) => group.entries.push(entry),
            None => groups.push(Group {
                name,
                entries: vec![entry],
            }),
        }
    }
    groups
}

#[must_use]
pub fn layer_in(keymap: &Keymap, combo: Combo) -> Layer {
    layer_where(keymap, combo, &|spec| spec.action.is_some())
}

#[must_use]
pub fn layer_where(keymap: &Keymap, combo: Combo, is_handled: IsHandled) -> Layer {
    let is_ctrl = combo == Combo::CTRL;
    Layer {
        combo,
        groups: grouped(entries(keymap, combo, is_handled)),
        more: if is_ctrl {
            entries(keymap, combo.with_shift(), is_handled)
        } else {
            Vec::new()
        },
    }
}
