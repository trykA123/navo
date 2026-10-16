mod actions;
mod filter;
mod rows;
mod selection;
mod text;

pub use actions::ferrite_actions;
pub use rows::{ItemRow, Row, Section, rare_rows, rows, submenu_rows};
pub use selection::{Choice, Selection};
pub use text::{CleanText, clean_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FerriteAction {
    Open,
    View,
    Edit,
    Copy,
    Move,
    Rename,
    Delete,
    CopyPath,
    Properties,
    ClassicMenu,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemId {
    Ferrite(FerriteAction),
    Shell { command: u32, verb: Option<String> },
    Action(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Ferrite,
    Open,
    Share,
    Edit,
    Extensions,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submenu {
    Filled(Vec<MenuItem>),
    Lazy,
    Loading,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Command,
    Submenu(Submenu),
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: ItemId,
    pub text: String,
    pub access_key: Option<char>,
    pub accelerator: Option<String>,
    pub icon: Option<Icon>,
    pub enabled: bool,
    pub checked: bool,
    pub default: bool,
    pub kind: ItemKind,
    pub group: Group,
    pub hidden: bool,
    pub rare: bool,
}

impl MenuItem {
    #[must_use]
    pub fn new(id: ItemId, raw_text: &str, group: Group) -> Self {
        let clean = clean_text(raw_text);
        Self {
            id,
            text: clean.text,
            access_key: clean.access_key,
            accelerator: clean.accelerator,
            icon: None,
            enabled: true,
            checked: false,
            default: false,
            kind: ItemKind::Command,
            group,
            hidden: false,
            rare: false,
        }
    }

    #[must_use]
    pub fn separator(group: Group) -> Self {
        Self {
            kind: ItemKind::Separator,
            ..Self::new(
                ItemId::Shell {
                    command: 0,
                    verb: None,
                },
                "",
                group,
            )
        }
    }

    #[must_use]
    pub fn is_separator(&self) -> bool {
        self.kind == ItemKind::Separator
    }

    #[must_use]
    pub fn is_ferrite(&self) -> bool {
        self.group == Group::Ferrite
    }

    #[must_use]
    pub fn is_submenu(&self) -> bool {
        matches!(self.kind, ItemKind::Submenu(_))
    }

    #[must_use]
    pub fn children(&self) -> Option<&[MenuItem]> {
        match &self.kind {
            ItemKind::Submenu(Submenu::Filled(items)) => Some(items),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellMenu {
    pub items: Vec<MenuItem>,
}

impl ShellMenu {
    #[must_use]
    pub fn item(&self, path: &[usize]) -> Option<&MenuItem> {
        let (first, rest) = path.split_first()?;
        let mut item = self.items.get(*first)?;
        for index in rest {
            item = item.children()?.get(*index)?;
        }
        Some(item)
    }

    pub fn item_mut(&mut self, path: &[usize]) -> Option<&mut MenuItem> {
        let (first, rest) = path.split_first()?;
        let mut item = self.items.get_mut(*first)?;
        for index in rest {
            item = match &mut item.kind {
                ItemKind::Submenu(Submenu::Filled(children)) => children.get_mut(*index)?,
                _ => return None,
            };
        }
        Some(item)
    }

    pub fn set_submenu(&mut self, path: &[usize], submenu: Submenu) -> bool {
        match self.item_mut(path) {
            Some(item) if item.is_submenu() => {
                item.kind = ItemKind::Submenu(submenu);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests;
