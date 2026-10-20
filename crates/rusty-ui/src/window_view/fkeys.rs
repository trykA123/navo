use super::FerriteWindow;
use crate::commands::{Keymap, normal_key};
use crate::fkey_bar::{FKeyBar, KeyLayer};
use crate::modifier_hints::HintsMode;
use crate::prelude::*;
use crate::settings::WindowParts;
use crate::tokens::Theme;

pub type FKey = (&'static str, &'static str);

const NAMES: [&str; 10] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

const PREFIXES: [&str; 4] = ["", "shift-", "ctrl-", "alt-"];

const SLOTS: [(&str, &str); 18] = [
    ("files.rename", "Rename"),
    ("files.view", "View"),
    ("files.edit", "Edit"),
    ("files.copy", "Copy"),
    ("files.move", "Move"),
    ("files.new_folder", "New folder"),
    ("files.new_file", "New file"),
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
    ("files.context_menu", "Context"),
];

const SETTINGS_SLOTS: [(&str, &str); 3] = [
    ("settings.edit_value", "Edit value"),
    ("settings.show_in_file", "TOML line"),
    ("app_menu.toggle", "Menu"),
];

pub type Labels = [[&'static str; 4]; 10];

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

fn labels_of(keymap: &Keymap, slots: &[(&str, &'static str)]) -> Labels {
    let mut labels = [[""; 4]; 10];
    for &(command, label) in slots {
        for key in keymap.keys_of(command) {
            place(&mut labels, key, label);
        }
    }
    labels
}

#[must_use]
pub fn key_labels_in(keymap: &Keymap) -> Labels {
    labels_of(keymap, &SLOTS)
}

#[must_use]
pub fn settings_labels_in(keymap: &Keymap) -> Labels {
    labels_of(keymap, &SETTINGS_SLOTS)
}

#[must_use]
pub fn keys_in_layer(labels: &Labels, layer: Option<KeyLayer>) -> [FKey; 10] {
    let slot = layer_slot(layer);
    std::array::from_fn(|index| (NAMES[index], labels[index][slot]))
}

#[must_use]
pub fn layer_keys_in(keymap: &Keymap, layer: Option<KeyLayer>) -> [FKey; 10] {
    keys_in_layer(&key_labels_in(keymap), layer)
}

#[must_use]
pub fn layer_keys(layer: Option<KeyLayer>) -> [FKey; 10] {
    layer_keys_in(Keymap::builtin(), layer)
}

#[must_use]
pub fn key_labels() -> Labels {
    key_labels_in(Keymap::builtin())
}

pub const DELETE_WARNING: &str = "Delete for good";

#[must_use]
pub fn delete_warning_in(keymap: &Keymap) -> Option<(Option<KeyLayer>, usize)> {
    let (index, layer) = keymap.keys_of("files.delete").find_map(cell_of)?;
    let layer = match layer {
        0 => None,
        1 => Some(KeyLayer::Shift),
        2 => Some(KeyLayer::Ctrl),
        _ => Some(KeyLayer::Alt),
    };
    Some((layer, index))
}

impl FerriteWindow {
    fn hides_fkeys(&self) -> bool {
        self.viewer.is_none() && !self.settings.is_open() && !WindowParts::of(&self.values).fkey_bar
    }

    fn shows_job_pill(&self) -> bool {
        self.job_ui.is_pill && !self.job_infos().is_empty()
    }

    pub(super) fn fkey_strip_height(&self, t: &Theme) -> Pixels {
        if self.hides_fkeys() && !self.shows_job_pill() {
            px(0.)
        } else {
            t.spacing.fkey_height
        }
    }

    pub(super) fn fkey_bar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.hides_fkeys() {
            return Some(self.keys_bar(window, cx).into_any_element());
        }
        let strip = self.shows_job_pill().then(|| self.pill_strip(window, cx));
        strip.map(|strip| strip.into_any_element())
    }

    fn keys_bar(&self, window: &Window, cx: &mut Context<Self>) -> FKeyBar {
        let is_fixed = self.values.modifier_hints() == HintsMode::Off;
        let held = self.hints.held.filter(|_| !is_fixed);
        let bar = match &self.viewer {
            Some(viewer) => FKeyBar::new("fkeys", viewer.keys(cx)),
            None if self.settings.is_open() => {
                let labels = settings_labels_in(crate::commands::active(cx));
                FKeyBar::new("fkeys", keys_in_layer(&labels, held)).layers(held, labels)
            }
            None => {
                let keymap = crate::commands::active(cx);
                let bar = FKeyBar::new("fkeys", layer_keys_in(keymap, held))
                    .layers(held, key_labels_in(keymap));
                match delete_warning_in(keymap) {
                    Some((layer, index)) => bar.warn(layer, index, DELETE_WARNING),
                    None => bar,
                }
            }
        };
        self.with_job_pill(bar, window, cx)
    }
}
