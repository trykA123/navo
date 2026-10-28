use rusty_core::Tiles;
use serde::{Deserialize, Serialize};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use super::{SettingsError, parse};
use crate::text_layout::keep_layout;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LayoutSettings {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<LayoutPreset>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LayoutPreset {
    pub name: String,
    pub tiles: String,
}

impl LayoutSettings {
    pub(super) fn validate(&self, text: &str) -> Result<(), SettingsError> {
        if self.presets.is_empty() {
            return Ok(());
        }
        let document = toml_edit::Document::parse(text).map_err(|_| SettingsError::FileInvalid)?;
        for (index, preset) in self.presets.iter().enumerate() {
            let problem = if !(1..=40).contains(&preset.name.chars().count()) {
                Some((
                    "name",
                    "layout preset names must contain 1–40 characters".to_owned(),
                ))
            } else if index >= 20 {
                Some(("name", "at most 20 layout presets are allowed".to_owned()))
            } else if self.presets[..index].iter().any(|p| p.name == preset.name) {
                Some(("name", "layout preset names must be unique".to_owned()))
            } else {
                Tiles::parse(&preset.tiles)
                    .err()
                    .map(|error| ("tiles", error.to_string()))
            };
            if let Some((key, message)) = problem {
                let offset = document
                    .get("layout")
                    .and_then(|item| item.get("presets"))
                    .and_then(|item| item.get(index))
                    .and_then(|item| item.get(key).and_then(Item::span).or_else(|| item.span()))
                    .map_or(0, |span| span.start);
                return Err(super::parse::invalid_value(text, offset, message));
            }
        }
        Ok(())
    }
}

pub fn save_layout_preset(text: &str, name: &str, tiles: &str) -> Result<String, SettingsError> {
    let (mut settings, _) = parse(text)?;
    if settings
        .layout
        .presets
        .iter()
        .any(|preset| preset.name == name)
    {
        return Err(SettingsError::Refused);
    }
    settings.layout.presets.push(LayoutPreset {
        name: name.to_owned(),
        tiles: tiles.to_owned(),
    });
    let candidate = super::to_toml(&settings);
    settings.layout.validate(&candidate)?;
    keep_layout(text, |plain| {
        let mut document: DocumentMut = plain.parse().map_err(|_| SettingsError::FileInvalid)?;
        let layout = document.entry("layout").or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        let presets = layout
            .as_table_like_mut()
            .ok_or(SettingsError::FileInvalid)?
            .entry("presets")
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
        let mut preset = Table::new();
        preset.insert("name", toml_edit::value(name));
        preset.insert("tiles", toml_edit::value(tiles));
        if let Some(tables) = presets.as_array_of_tables_mut() {
            tables.push(preset);
        } else if let Some(array) = presets.as_array_mut() {
            array.push(preset.into_inline_table());
        } else {
            return Err(SettingsError::FileInvalid);
        }
        Ok(with_final_newline(plain, document.to_string()))
    })
}

pub fn replace_layout_preset(text: &str, name: &str, tiles: &str) -> Result<String, SettingsError> {
    let (mut settings, _) = parse(text)?;
    let index = settings
        .layout
        .presets
        .iter()
        .position(|preset| preset.name == name)
        .ok_or(SettingsError::Refused)?;
    tiles.clone_into(&mut settings.layout.presets[index].tiles);
    settings.layout.validate(&super::to_toml(&settings))?;
    keep_layout(text, |plain| {
        let mut document: DocumentMut = plain.parse().map_err(|_| SettingsError::FileInvalid)?;
        let presets = document
            .get_mut("layout")
            .and_then(|layout| layout.get_mut("presets"))
            .ok_or(SettingsError::FileInvalid)?;
        let slot = if let Some(tables) = presets.as_array_of_tables_mut() {
            tables
                .get_mut(index)
                .and_then(|table| table.get_mut("tiles"))
                .and_then(Item::as_value_mut)
        } else {
            presets
                .as_array_mut()
                .and_then(|array| array.get_mut(index))
                .and_then(Value::as_inline_table_mut)
                .and_then(|table| table.get_mut("tiles"))
        };
        let slot = slot.ok_or(SettingsError::FileInvalid)?;
        let decor = slot.decor().clone();
        *slot = tiles.into();
        *slot.decor_mut() = decor;
        Ok(with_final_newline(plain, document.to_string()))
    })
}

pub fn remove_layout_preset(text: &str, name: &str) -> Result<String, SettingsError> {
    let (settings, _) = parse(text)?;
    let index = settings
        .layout
        .presets
        .iter()
        .position(|preset| preset.name == name)
        .ok_or(SettingsError::Refused)?;
    keep_layout(text, |plain| {
        let mut document: DocumentMut = plain.parse().map_err(|_| SettingsError::FileInvalid)?;
        let layout = document
            .get_mut("layout")
            .and_then(Item::as_table_like_mut)
            .ok_or(SettingsError::FileInvalid)?;
        let presets = layout
            .get_mut("presets")
            .ok_or(SettingsError::FileInvalid)?;
        if let Some(tables) = presets.as_array_of_tables_mut() {
            tables.remove(index);
            if tables.is_empty() {
                layout.remove("presets");
            }
        } else if let Some(array) = presets.as_array_mut() {
            array.remove(index);
        } else {
            return Err(SettingsError::FileInvalid);
        }
        Ok(with_final_newline(plain, document.to_string()))
    })
}

fn with_final_newline(original: &str, mut edited: String) -> String {
    if !original.ends_with('\n') && edited.ends_with('\n') {
        edited.pop();
    }
    edited
}
