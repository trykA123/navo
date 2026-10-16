use gpui_kit::SharedString;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct QuickChange {
    pub theme: Option<ThemeChoice>,
    pub density: Option<Density>,
    pub show_hidden: Option<bool>,
}

impl QuickChange {
    #[must_use]
    pub fn of(setting: Setting, values: QuickSettings) -> Self {
        match setting {
            Setting::Theme => Self {
                theme: Some(values.theme),
                ..Self::default()
            },
            Setting::Density => Self {
                density: Some(values.density),
                ..Self::default()
            },
            Setting::HiddenFiles => Self {
                show_hidden: Some(values.show_hidden),
                ..Self::default()
            },
        }
    }

    #[must_use]
    pub fn merged(self, newer: Self) -> Self {
        Self {
            theme: newer.theme.or(self.theme),
            density: newer.density.or(self.density),
            show_hidden: newer.show_hidden.or(self.show_hidden),
        }
    }

    #[must_use]
    pub fn applied_to(self, values: QuickSettings) -> QuickSettings {
        QuickSettings {
            theme: self.theme.unwrap_or(values.theme),
            density: self.density.unwrap_or(values.density),
            show_hidden: self.show_hidden.unwrap_or(values.show_hidden),
        }
    }
}

pub(super) enum Trailing {
    Keys(&'static str),
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
        label: "Command palette",
        trailing: Trailing::Keys("Ctrl+K"),
    },
    RowSpec {
        command: MenuCommand::NewWindow,
        icon: IconName::AppWindow,
        label: "New window",
        trailing: Trailing::Keys("Ctrl+N"),
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
        trailing: Trailing::Keys("F1"),
    },
    RowSpec {
        command: MenuCommand::Quit,
        icon: IconName::Power,
        label: "Quit",
        trailing: Trailing::Keys("Alt+F4"),
    },
    RowSpec {
        command: MenuCommand::AllSettings,
        icon: IconName::SlidersHorizontal,
        label: "All settings",
        trailing: Trailing::Keys("Ctrl+,"),
    },
];

impl MenuCommand {
    pub fn is_available(self) -> bool {
        matches!(self, Self::Quit)
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

pub(super) fn row_labels() -> [&'static str; ROWS.len()] {
    ROWS.map(|spec| spec.label)
}
