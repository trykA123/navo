use gpui_kit::SharedString;

use super::fields::{Control, FIELDS, Field, FieldSpec};
use crate::app_menu::{QuickSettings, Setting};
use crate::file_list::{SortBy, SortKey};
use crate::tokens::{Density, ThemeChoice};

const THEME_ORDER: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];
const DENSITY_ORDER: [Density; 3] = [Density::Compact, Density::Normal, Density::Comfortable];
const SORT_ORDER: [SortBy; 4] = [SortBy::Name, SortBy::Size, SortBy::Date, SortBy::Ext];
const SHOWN: usize = 0;
const HIDDEN: usize = 1;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Value {
    Flag(bool),
    Pick(usize),
    Text(SharedString),
    Number(i64),
    Unset,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Change {
    pub field: Field,
    pub value: Value,
}

impl Change {
    pub fn key(&self) -> &'static str {
        self.field.spec().key
    }

    pub fn theme(theme: ThemeChoice) -> Self {
        Self::pick(Field::Theme, &THEME_ORDER, theme)
    }

    pub fn density(density: Density) -> Self {
        Self::pick(Field::Density, &DENSITY_ORDER, density)
    }

    pub fn show_hidden(shown: bool) -> Self {
        Self {
            field: Field::HiddenFiles,
            value: Value::Pick(if shown { SHOWN } else { HIDDEN }),
        }
    }

    pub fn text(field: Field, text: impl Into<SharedString>) -> Self {
        Self {
            field,
            value: Value::Text(text.into()),
        }
    }

    fn pick<T: PartialEq>(field: Field, order: &[T], wanted: T) -> Self {
        let index = order.iter().position(|item| *item == wanted).unwrap_or(0);
        Self {
            field,
            value: Value::Pick(index),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Values {
    values: Vec<Value>,
}

fn default_of(spec: &FieldSpec) -> Value {
    let text = |text: &'static str| Value::Text(SharedString::new_static(text));
    match spec.field {
        Field::Density => Value::Pick(1),
        Field::LightTheme => text("Ferrite Light"),
        Field::DarkTheme => text("Ferrite Dark"),
        Field::Font => text("Inter"),
        Field::FontSize => Value::Number(13),
        Field::FoldersFirst | Field::SpaceMeasuresFolders => Value::Flag(true),
        Field::TerminalProvider => text("windows-terminal"),
        _ => match spec.control {
            Control::Choices(..) => Value::Pick(0),
            Control::Toggle => Value::Flag(false),
            Control::Text => text(""),
            Control::Number => Value::Number(0),
        },
    }
}

impl Default for Values {
    fn default() -> Self {
        Self::from_fn(default_of)
    }
}

impl Values {
    pub fn from_fn(mut value_of: impl FnMut(&FieldSpec) -> Value) -> Self {
        Self {
            values: FIELDS.iter().map(&mut value_of).collect(),
        }
    }

    pub fn get(&self, field: Field) -> &Value {
        &self.values[field.index()]
    }

    #[must_use]
    pub fn with(&self, change: &Change) -> Self {
        let mut values = self.clone();
        values.values[change.field.index()] = match &change.value {
            Value::Unset => default_of(change.field.spec()),
            value => value.clone(),
        };
        values
    }

    pub fn change_of(&self, field: Field) -> Change {
        Change {
            field,
            value: self.get(field).clone(),
        }
    }

    fn pick(&self, field: Field) -> usize {
        match self.get(field) {
            Value::Pick(index) => *index,
            _ => 0,
        }
    }

    pub fn theme(&self) -> ThemeChoice {
        THEME_ORDER
            .get(self.pick(Field::Theme))
            .copied()
            .unwrap_or_default()
    }

    pub fn density(&self) -> Density {
        DENSITY_ORDER
            .get(self.pick(Field::Density))
            .copied()
            .unwrap_or_default()
    }

    pub fn show_hidden(&self) -> bool {
        self.pick(Field::HiddenFiles) == SHOWN
    }

    fn flag(&self, field: Field) -> bool {
        matches!(self.get(field), Value::Flag(true))
    }

    pub fn show_system(&self) -> bool {
        self.flag(Field::ShowSystem)
    }

    pub fn folders_first(&self) -> bool {
        self.flag(Field::FoldersFirst)
    }

    pub fn sort_key(&self) -> SortKey {
        SortKey {
            by: SORT_ORDER
                .get(self.pick(Field::Sort))
                .copied()
                .unwrap_or_default(),
            descending: self.flag(Field::SortDescending),
        }
    }

    pub fn space_measures_folders(&self) -> bool {
        self.flag(Field::SpaceMeasuresFolders)
    }

    pub fn patterns_select_folders(&self) -> bool {
        self.flag(Field::PatternsSelectFolders)
    }

    pub fn editor_path(&self) -> Option<String> {
        match self.get(Field::EditorPath) {
            Value::Text(text) if !text.is_empty() => Some(text.to_string()),
            _ => None,
        }
    }

    pub fn quick(&self) -> QuickSettings {
        QuickSettings {
            theme: self.theme(),
            density: self.density(),
            show_hidden: self.show_hidden(),
        }
    }

    pub fn toggled(&self, setting: Setting) -> Change {
        let toggled = self.quick().toggled(setting);
        match setting {
            Setting::Theme => Change::theme(toggled.theme),
            Setting::Density => Change::density(toggled.density),
            Setting::HiddenFiles => Change::show_hidden(toggled.show_hidden),
        }
    }
}
