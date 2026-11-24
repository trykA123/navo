//! Builds a ribbon layer from the keymap: entries with short labels, grouped per layer,
//! the F-row entries apart, and the counts of the combo layers one modifier away.

use gpui_kit::SharedString;
use navo_l10n::{catalog, tr};

use super::combo::Combo;
use super::groups::{group_of, is_danger, place_of};
use super::runs::folded;
use crate::commands::{Bind, CommandSpec, GLOBAL, Keymap};
use crate::host_os::parse_keystroke;

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
    /// The full command name: the accessible name and the tooltip.
    pub label: SharedString,
    /// The short label the ribbon shows.
    pub short: SharedString,
    pub keys: Vec<SharedString>,
    pub is_danger: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Group {
    pub name: SharedString,
    pub entries: Vec<Entry>,
}

/// The size of one group, for fitting the band: its entries and its widest first key cap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shape {
    pub entries: usize,
    pub cap_chars: usize,
}

/// A layer one modifier away, for the header: "+ Shift 8 more".
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct More {
    pub modifier: &'static str,
    pub count: usize,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layer {
    pub combo: Combo,
    pub groups: Vec<Group>,
    /// Entries on F1 to F10, one key each, in key order.
    pub fkeys: Vec<Entry>,
    pub more: Vec<More>,
    /// Group shapes of the Ctrl, Shift and Alt layers and this one; the card takes the
    /// row count at which all of them fit, so its height never jumps.
    pub shapes: Vec<Vec<Shape>>,
}

impl Layer {
    /// Number of keys in the file list: group entries plus F-row entries.
    #[must_use]
    pub fn count(&self) -> usize {
        let grouped: usize = self.groups.iter().map(|group| group.entries.len()).sum();
        grouped + self.fkeys.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.fkeys.is_empty()
    }

    #[must_use]
    pub fn rows(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.entries.len())
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
        .filter_map(|key| parse_keystroke(key).ok())
        .filter(|stroke| combo.matches(&stroke.modifiers))
        .map(|stroke| key_cap(&stroke.key).into())
        .collect()
}

pub type IsHandled<'a> = &'a dyn Fn(&CommandSpec) -> bool;

/// The layers one modifier away from a single-modifier layer, in header order.
const NEIGHBOURS: [(Combo, [(&str, Combo); 2]); 3] = [
    (
        Combo::CTRL,
        [
            ("Shift", Combo::CTRL_SHIFT),
            ("Alt", Combo::new(true, true, false)),
        ],
    ),
    (
        Combo::SHIFT,
        [
            ("Ctrl", Combo::CTRL_SHIFT),
            ("Alt", Combo::new(false, true, true)),
        ],
    ),
    (
        Combo::ALT,
        [
            ("Shift", Combo::new(false, true, true)),
            ("Ctrl", Combo::new(true, true, false)),
        ],
    ),
];

/// Looks up `<id><suffix>` and falls back to the next suffix, then to the id itself.
fn label_of(id: &str, suffixes: &[&str]) -> SharedString {
    suffixes
        .iter()
        .map(|suffix| format!("{id}{suffix}"))
        .find(|candidate| catalog().has(candidate))
        .map_or_else(|| tr(id), |candidate| tr(&candidate))
        .into()
}

/// The F-key number of a key cap, for F1 to F10 only.
fn fkey_number(cap: &str) -> Option<u8> {
    let number: u8 = cap.strip_prefix('F')?.parse().ok()?;
    (1..=10).contains(&number).then_some(number)
}

/// Entries on a combo, merged per command with duplicate caps removed.
fn merged(keymap: &Keymap, combo: Combo, is_handled: IsHandled) -> Vec<(Entry, &'static str)> {
    let mut entries: Vec<(Entry, &'static str)> = Vec::new();
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
        let Some((entry, _)) = entries.iter_mut().find(|(entry, _)| entry.command == name) else {
            let entry = Entry {
                command: name,
                label: tr(bind.spec.label).into(),
                short: label_of(bind.spec.label, &["-short"]),
                keys: caps,
                is_danger: is_danger(name),
            };
            entries.push((entry, bind.spec.label));
            continue;
        };
        entry.keys.extend(caps);
    }
    for (entry, _) in &mut entries {
        let mut seen = Vec::new();
        entry.keys.retain(|key| {
            let is_new = !seen.contains(key);
            seen.push(key.clone());
            is_new
        });
    }
    entries
}

/// Splits a combo's entries into the grouped list (folded runs) and the F-row.
fn entries(keymap: &Keymap, combo: Combo, is_handled: IsHandled) -> (Vec<Entry>, Vec<Entry>) {
    let mut listed = Vec::new();
    let mut fkeys: Vec<(u8, Entry)> = Vec::new();
    for (mut entry, label_id) in merged(keymap, combo, is_handled) {
        for key in &entry.keys {
            let Some(number) = fkey_number(key) else {
                continue;
            };
            let short = label_of(label_id, &["-fkey", "-short"]);
            let keys = vec![key.clone()];
            fkeys.push((
                number,
                Entry {
                    short,
                    keys,
                    ..entry.clone()
                },
            ));
        }
        entry.keys.retain(|key| fkey_number(key).is_none());
        if !entry.keys.is_empty() {
            listed.push(entry);
        }
    }
    listed.sort_by_key(|entry| place_of(entry.command, combo));
    fkeys.sort_by_key(|(number, _)| *number);
    (
        folded(&listed),
        fkeys.into_iter().map(|(_, entry)| entry).collect(),
    )
}

fn grouped(entries: Vec<Entry>) -> Vec<Group> {
    let mut groups: Vec<(&'static str, Group)> = Vec::new();
    for entry in entries {
        let id = group_of(entry.command);
        match groups.last_mut().filter(|(group, _)| *group == id) {
            Some((_, group)) => group.entries.push(entry),
            None => groups.push((
                id,
                Group {
                    name: tr(id).into(),
                    entries: vec![entry],
                },
            )),
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

/// Counts of the layers one modifier away; empty ones are left out.
fn more_of(keymap: &Keymap, combo: Combo, is_handled: IsHandled) -> Vec<More> {
    let Some((_, neighbours)) = NEIGHBOURS.iter().find(|(own, _)| *own == combo) else {
        return Vec::new();
    };
    neighbours
        .iter()
        .filter_map(|&(modifier, other)| {
            let (listed, fkeys) = entries(keymap, other, is_handled);
            let count = listed.len() + fkeys.len();
            (count > 0).then_some(More { modifier, count })
        })
        .collect()
}

/// The shape of each group: entry count and the widest first key cap, in characters.
fn shape_of(groups: &[Group]) -> Vec<Shape> {
    let first_cap = |entry: &Entry| entry.keys.first().map_or(0, |key| key.chars().count());
    groups
        .iter()
        .map(|group| Shape {
            entries: group.entries.len(),
            cap_chars: group.entries.iter().map(first_cap).max().unwrap_or(0),
        })
        .collect()
}

#[must_use]
pub fn layer_in(keymap: &Keymap, combo: Combo) -> Layer {
    layer_where(keymap, combo, &|spec| spec.action.is_some())
}

#[must_use]
pub fn layer_where(keymap: &Keymap, combo: Combo, is_handled: IsHandled) -> Layer {
    let (listed, fkeys) = entries(keymap, combo, is_handled);
    let groups = grouped(listed);
    let peers = [Combo::CTRL, Combo::SHIFT, Combo::ALT]
        .into_iter()
        .filter(|peer| *peer != combo);
    let mut shapes: Vec<Vec<Shape>> = peers
        .map(|peer| shape_of(&grouped(entries(keymap, peer, is_handled).0)))
        .collect();
    shapes.push(shape_of(&groups));
    Layer {
        combo,
        groups,
        fkeys,
        more: more_of(keymap, combo, is_handled),
        shapes,
    }
}
