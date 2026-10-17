use crate::commands::{Keymap, normal_key};
use crate::fkey_bar::KeyLayer;

pub type FKey = (&'static str, &'static str);

const NAMES: [&str; 10] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

const PREFIXES: [&str; 4] = ["", "shift-", "ctrl-", "alt-"];

const SLOTS: [(&str, &str); 16] = [
    ("files.rename", "Rename"),
    ("files.view", "View"),
    ("files.edit", "Edit"),
    ("files.copy", "Copy"),
    ("files.move", "Move"),
    ("files.new_folder", "New folder"),
    ("files.recycle", "Delete"),
    ("app_menu.toggle", "Menu"),
    ("files.edit_with", "Edit with"),
    ("files.delete", "Delete"),
    ("files.sort_by_name", "By name"),
    ("files.sort_by_extension", "By ext"),
    ("files.sort_by_date", "By date"),
    ("files.sort_by_size", "By size"),
    ("pane.drive_menu_first", "Left drive"),
    ("pane.drive_menu_second", "Right drive"),
];

// F9 has no command yet; its cap is reserved for the drawer.
const RESERVED: [(&str, &str); 1] = [("f9", "Drawer")];

type Labels = [[&'static str; 4]; 10];

fn cell_of(key: &str) -> Option<(usize, usize)> {
    let normal = normal_key(key)?;
    PREFIXES.iter().enumerate().find_map(|(layer, prefix)| {
        let number: usize = normal
            .strip_prefix(prefix)?
            .strip_prefix('f')?
            .parse()
            .ok()?;
        (1..=NAMES.len())
            .contains(&number)
            .then_some((number - 1, layer))
    })
}

fn place(labels: &mut Labels, key: &str, label: &'static str) {
    let cell = cell_of(key).and_then(|(index, layer)| labels.get_mut(index)?.get_mut(layer));
    if let Some(cell) = cell.filter(|cell| cell.is_empty()) {
        *cell = label;
    }
}

fn layer_slot(layer: Option<KeyLayer>) -> usize {
    match layer {
        None => 0,
        Some(KeyLayer::Shift) => 1,
        Some(KeyLayer::Ctrl) => 2,
        Some(KeyLayer::Alt) => 3,
    }
}

#[must_use]
pub fn key_labels_in(keymap: &Keymap) -> Labels {
    let mut labels = [[""; 4]; 10];
    for (command, label) in SLOTS {
        for key in keymap.keys_of(command) {
            place(&mut labels, key, label);
        }
    }
    for (key, label) in RESERVED {
        place(&mut labels, key, label);
    }
    labels
}

#[must_use]
pub fn layer_keys_in(keymap: &Keymap, layer: Option<KeyLayer>) -> [FKey; 10] {
    let labels = key_labels_in(keymap);
    let slot = layer_slot(layer);
    std::array::from_fn(|index| (NAMES[index], labels[index][slot]))
}

#[must_use]
pub fn layer_keys(layer: Option<KeyLayer>) -> [FKey; 10] {
    layer_keys_in(Keymap::builtin(), layer)
}

#[must_use]
pub fn key_labels() -> Labels {
    key_labels_in(Keymap::builtin())
}

pub const KEY_WARNING: (KeyLayer, usize, &str) = (KeyLayer::Shift, 7, "Delete for good");
