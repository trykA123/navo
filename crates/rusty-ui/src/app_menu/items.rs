use gpui_kit::SharedString;

use crate::primitives::IconName;
use crate::tokens::ThemeChoice;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    Theme,
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
    Keys(&'static str),
    Submenu,
}

pub(super) struct RowSpec {
    pub command: MenuCommand,
    pub icon: IconName,
    pub label: &'static str,
    pub trailing: Trailing,
}

pub(super) const TILES: [Setting; 1] = [Setting::Theme];
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

pub(super) fn tile_face(setting: Setting, theme: ThemeChoice) -> TileFace {
    match setting {
        Setting::Theme => TileFace {
            icon: IconName::Contrast,
            label: "Theme",
            value: theme.label().into(),
            is_on: theme != ThemeChoice::System,
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
