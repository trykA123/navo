use gpui_kit::SharedString;

use crate::commands::{Keymap, for_menu, shown};
use crate::primitives::IconName;
use crate::tokens::{Density, ThemeChoice};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    Theme,
    Density,
    HiddenFiles,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct QuickSettings {
    pub theme: ThemeChoice,
    pub density: Density,
    pub show_hidden: bool,
}

impl QuickSettings {
    #[must_use]
    pub fn toggled(self, setting: Setting) -> Self {
        match setting {
            Setting::Theme => Self {
                theme: self.theme.next(),
                ..self
            },
            Setting::Density => Self {
                density: self.density.next(),
                ..self
            },
            Setting::HiddenFiles => Self {
                show_hidden: !self.show_hidden,
                ..self
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuCommand {
    CommandPalette,
    NewWindow,
    Workspaces,
    Help,
    Quit,
    AllSettings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Tile(Setting),
    Row(MenuCommand),
}

pub(super) enum Trailing {
    Keys,
    Submenu,
}

pub(super) struct RowSpec {
    pub command: MenuCommand,
    pub icon: IconName,
    pub label: &'static str,
    pub trailing: Trailing,
}

pub(super) const TILES: [Setting; 3] = [Setting::Theme, Setting::Density, Setting::HiddenFiles];
pub(super) const COLUMNS: usize = 3;

pub(super) const ROWS: [RowSpec; 6] = [
    RowSpec {
        command: MenuCommand::CommandPalette,
        icon: IconName::Command,
        label: "Command center",
        trailing: Trailing::Keys,
    },
    RowSpec {
        command: MenuCommand::NewWindow,
        icon: IconName::AppWindow,
        label: "New window",
        trailing: Trailing::Keys,
    },
    RowSpec {
        command: MenuCommand::Workspaces,
        icon: IconName::Layers,
        label: "Workspaces",
        trailing: Trailing::Submenu,
    },
    RowSpec {
        command: MenuCommand::Help,
        icon: IconName::CircleQuestionMark,
        label: "Help and key map",
        trailing: Trailing::Keys,
    },
    RowSpec {
        command: MenuCommand::Quit,
        icon: IconName::Power,
        label: "Quit",
        trailing: Trailing::Keys,
    },
    RowSpec {
        command: MenuCommand::AllSettings,
        icon: IconName::SlidersHorizontal,
        label: "All settings",
        trailing: Trailing::Keys,
    },
];

impl MenuCommand {
    pub fn is_available(self) -> bool {
        matches!(self, Self::Quit | Self::AllSettings)
    }

    pub(super) fn is_primary(self) -> bool {
        matches!(self, Self::AllSettings)
    }
}

pub(super) struct TileFace {
    pub icon: IconName,
    pub label: &'static str,
    pub value: SharedString,
    pub is_on: bool,
}

pub(super) fn tile_face(setting: Setting, settings: QuickSettings) -> TileFace {
    match setting {
        Setting::Theme => TileFace {
            icon: IconName::Contrast,
            label: "Theme",
            value: settings.theme.label().into(),
            is_on: settings.theme != ThemeChoice::System,
        },
        Setting::Density => TileFace {
            icon: IconName::Rows3,
            label: "Density",
            value: settings.density.label().into(),
            is_on: settings.density != Density::Normal,
        },
        Setting::HiddenFiles => TileFace {
            icon: IconName::EyeOff,
            label: "Hidden files",
            value: if settings.show_hidden {
                "Shown"
            } else {
                "Hidden"
            }
            .into(),
            is_on: settings.show_hidden,
        },
    }
}

pub const ITEM_COUNT: usize = TILES.len() + ROWS.len();

#[must_use]
pub fn item_at(index: usize) -> Option<Item> {
    match index.checked_sub(TILES.len()) {
        None => TILES.get(index).copied().map(Item::Tile),
        Some(row) => ROWS.get(row).map(|spec| Item::Row(spec.command)),
    }
}

#[must_use]
pub fn index_of(item: Item) -> Option<usize> {
    (0..ITEM_COUNT).find(|index| item_at(*index) == Some(item))
}

#[must_use]
pub(super) fn row_keys(keymap: &Keymap, command: MenuCommand) -> Option<String> {
    keymap.keys_of(for_menu(command)).find_map(shown)
}

pub(super) fn row_labels() -> [&'static str; ROWS.len()] {
    ROWS.map(|spec| spec.label)
}
