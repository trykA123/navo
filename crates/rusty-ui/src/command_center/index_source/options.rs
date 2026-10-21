use rusty_core::Location;
use rusty_index::table::Flags;

use crate::settings::{Field, Value, Values};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IndexMode {
    #[default]
    Auto,
    Service,
    Walk,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexOptions {
    pub mode: IndexMode,
    pub drives: Vec<String>,
    pub exclude: Vec<String>,
    pub network: bool,
    pub removable: bool,
    pub show_hidden: bool,
    pub show_system: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self::of(&Values::default())
    }
}

impl IndexOptions {
    #[must_use]
    pub fn of(values: &Values) -> Self {
        let list = |field| match values.get(field) {
            Value::Text(text) => text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        let mode = match values.get(Field::SearchIndex) {
            Value::Pick(1) => IndexMode::Service,
            Value::Pick(2) => IndexMode::Walk,
            Value::Pick(3) => IndexMode::Off,
            _ => IndexMode::Auto,
        };
        Self {
            mode,
            drives: list(Field::SearchDrives),
            exclude: list(Field::SearchExclude),
            network: values.flag(Field::SearchNetwork),
            removable: values.flag(Field::SearchRemovable),
            show_hidden: values.show_hidden(),
            show_system: values.show_system(),
        }
    }

    pub fn admits(&self, location: &Location, flags: u16) -> bool {
        if (!self.show_hidden && flags & Flags::HIDDEN.bits() != 0)
            || (!self.show_system && flags & Flags::SYSTEM.bits() != 0)
            || (!self.network && location.as_str().starts_with(r"\\"))
        {
            return false;
        }
        let parents = || std::iter::successors(Some(location.clone()), Location::parent);
        let requested = self.drives.is_empty()
            || self.drives.iter().any(|root| {
                let root = drive_root(root);
                Location::new(&root)
                    .is_ok_and(|root| parents().any(|parent| parent.same_place(&root)))
            });
        requested
            && !parents().any(|parent| {
                self.exclude.iter().any(|skip| {
                    parent.name().is_some_and(|name| name.to_text() == *skip)
                        || parent.as_str() == skip
                        || parent.path().to_text() == *skip
                })
            })
    }
}

pub fn drive_root(root: &str) -> String {
    if root.len() == 2 && root.as_bytes()[1] == b':' {
        format!("{root}\\")
    } else {
        root.to_owned()
    }
}
