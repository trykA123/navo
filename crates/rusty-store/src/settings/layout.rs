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

const MAX_PRESETS: usize = 20;

type Problem = (&'static str, String);

impl LayoutSettings {
    pub(super) fn drop_invalid(&mut self, text: &str) -> Vec<String> {
        let verdicts = verdicts(&self.presets);
        if verdicts.iter().all(Option::is_none) {
            return Vec::new();
        }
        let document = toml_edit::Document::parse(text).ok();
        let mut lines = Vec::new();
        let presets = std::mem::take(&mut self.presets);
        for (index, (preset, verdict)) in presets.into_iter().zip(verdicts).enumerate() {
            let Some((key, message)) = verdict else {
                self.presets.push(preset);
                continue;
            };
            let offset = document
                .as_ref()
                .and_then(|document| document.get("layout"))
                .and_then(|item| item.get("presets"))
                .and_then(|item| item.get(index))
                .and_then(|item| item.get(key).and_then(Item::span).or_else(|| item.span()))
                .map_or(0, |span| span.start);
            let line = super::parse::line_of(text, offset);
            lines.push(format!(
                "layout.presets[{}] {:?} (line {line}) was dropped: {message}",
                index + 1,
                preset.name
            ));
        }
        lines
    }
}

fn verdicts(presets: &[LayoutPreset]) -> Vec<Option<Problem>> {
    let mut kept: Vec<&str> = Vec::new();
    presets
        .iter()
        .map(|preset| {
            let problem = problem(&kept, &preset.name, &preset.tiles);
            if problem.is_none() {
                kept.push(&preset.name);
            }
            problem
        })
        .collect()
}

fn problem(kept: &[&str], name: &str, tiles: &str) -> Option<Problem> {
    if !(1..=40).contains(&name.chars().count()) {
        Some((
            "name",
            "layout preset names must contain 1–40 characters".to_owned(),
        ))
    } else if kept.contains(&name) {
        Some(("name", "layout preset names must be unique".to_owned()))
    } else if kept.len() >= MAX_PRESETS {
        Some(("name", "at most 20 layout presets are allowed".to_owned()))
    } else {
        Tiles::parse(tiles)
            .err()
            .map(|error| ("tiles", error.to_string()))
    }
}

fn document_index(text: &str, name: &str) -> Result<usize, SettingsError> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Raw {
        layout: LayoutSettings,
    }
    parse(text)?;
    let raw: Raw = toml::from_str(text).map_err(|_| SettingsError::FileInvalid)?;
    verdicts(&raw.layout.presets)
        .iter()
        .zip(&raw.layout.presets)
        .position(|(verdict, preset)| verdict.is_none() && preset.name == name)
        .ok_or(SettingsError::Refused)
}

fn refuse(text: &str, (_, message): Problem) -> SettingsError {
    super::parse::invalid_value(text, 0, message)
}

pub fn save_layout_preset(text: &str, name: &str, tiles: &str) -> Result<String, SettingsError> {
    let (settings, _) = parse(text)?;
    let names: Vec<&str> = settings
        .layout
        .presets
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    if names.contains(&name) {
        return Err(SettingsError::Refused);
    }
    if let Some(problem) = problem(&names, name, tiles) {
        return Err(refuse(text, problem));
    }
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
    let index = document_index(text, name)?;
    if let Err(error) = Tiles::parse(tiles) {
        return Err(refuse(text, ("tiles", error.to_string())));
    }
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
    let index = document_index(text, name)?;
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
