use crate::fkey_bar::KeyLayer;

pub type FKey = (&'static str, &'static str);

const BASE: [FKey; 10] = [
    ("F1", ""),
    ("F2", "Rename"),
    ("F3", "View"),
    ("F4", "Edit"),
    ("F5", "Copy"),
    ("F6", "Move"),
    ("F7", "New folder"),
    ("F8", "Delete"),
    ("F9", "Drawer"),
    ("F10", "Palette"),
];

const SHIFT: [FKey; 10] = [
    ("F1", ""),
    ("F2", ""),
    ("F3", ""),
    ("F4", "Edit with"),
    ("F5", ""),
    ("F6", "Rename"),
    ("F7", ""),
    ("F8", "Delete"),
    ("F9", ""),
    ("F10", ""),
];

const CTRL: [FKey; 10] = [
    ("F1", ""),
    ("F2", ""),
    ("F3", "By name"),
    ("F4", "By ext"),
    ("F5", "By date"),
    ("F6", "By size"),
    ("F7", ""),
    ("F8", ""),
    ("F9", ""),
    ("F10", ""),
];

const ALT: [FKey; 10] = [
    ("F1", "Left drive"),
    ("F2", "Right drive"),
    ("F3", ""),
    ("F4", ""),
    ("F5", ""),
    ("F6", ""),
    ("F7", ""),
    ("F8", ""),
    ("F9", ""),
    ("F10", ""),
];

#[must_use]
pub fn layer_keys(layer: Option<KeyLayer>) -> [FKey; 10] {
    match layer {
        None => BASE,
        Some(KeyLayer::Shift) => SHIFT,
        Some(KeyLayer::Ctrl) => CTRL,
        Some(KeyLayer::Alt) => ALT,
    }
}

#[must_use]
pub fn key_labels() -> [[&'static str; 4]; 10] {
    std::array::from_fn(|index| [BASE, SHIFT, CTRL, ALT].map(|layer| layer[index].1))
}

#[must_use]
pub fn key_warning(layer: Option<KeyLayer>) -> Option<(usize, &'static str)> {
    (layer == Some(KeyLayer::Shift)).then_some((7, "Delete for good"))
}
