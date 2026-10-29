use toml_edit::{DocumentMut, Item, TableLike, Value};

use super::SettingsError;
use crate::text_layout::{BOM, plain_text, restore_layout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Text(String),
    List(Vec<String>),
}

impl From<SettingValue> for Value {
    fn from(value: SettingValue) -> Self {
        match value {
            SettingValue::Bool(flag) => flag.into(),
            SettingValue::Integer(number) => number.into(),
            SettingValue::Text(text) => text.into(),
            SettingValue::List(items) => items.into_iter().collect::<toml_edit::Array>().into(),
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static SAVE_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) struct SaveDocument {
    original: String,
    document: DocumentMut,
    pub key_source: Option<(String, String)>,
}

impl SaveDocument {
    pub fn parse(text: &str, key: &str) -> Result<Self, SettingsError> {
        #[cfg(test)]
        SAVE_PARSES.with(|count| count.set(count.get() + 1));
        let plain = plain_text(text);
        let document = toml_edit::Document::parse(plain).map_err(|_| SettingsError::FileInvalid)?;
        let line = key
            .split('.')
            .try_fold(document.as_item(), |item, name| item.get(name))
            .and_then(Item::span)
            .and_then(|span| document.raw().get(..span.start))
            .map(|before| before.matches('\n').count());
        let document = document.into_mut();
        let key_source = key
            .split('.')
            .try_fold(document.as_item(), |item, name| item.get(name))
            .map(|item| {
                (
                    text.lines()
                        .nth(line.unwrap_or(0))
                        .unwrap_or_default()
                        .to_owned(),
                    item.to_string(),
                )
            });
        let source = Self {
            original: text.to_owned(),
            document,
            key_source,
        };
        source.settings().map_err(|_| SettingsError::FileInvalid)?;
        Ok(source)
    }

    pub fn settings(&self) -> Result<super::Settings, SettingsError> {
        let mut settings: super::Settings = toml_edit::de::from_document(self.document.clone())
            .map_err(|error| super::parse::invalid_value(&self.original, 0, error.to_string()))?;
        settings.layout.drop_invalid(&self.original);
        Ok(settings)
    }

    pub fn patch(&mut self, key: &str, value: Option<SettingValue>) -> Result<(), SettingsError> {
        patch(&mut self.document, key, value)?;
        upgrade_legacy_themes(&mut self.document);
        Ok(())
    }

    pub fn text(&self) -> String {
        restore_layout(&self.original, &self.document.to_string())
    }
}

fn upgrade_legacy_themes(document: &mut DocumentMut) {
    let Some(table) = document
        .get_mut("appearance")
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };
    for name in ["light-theme", "dark-theme"] {
        let Some(Item::Value(old)) = table.get_mut(name) else {
            continue;
        };
        let Some(new) = old.as_str().and_then(super::model::navo_theme_name) else {
            continue;
        };
        let mut value = Value::from(new);
        *value.decor_mut() = old.decor().clone();
        *old = value;
    }
}

fn patch(
    document: &mut DocumentMut,
    key: &str,
    value: Option<SettingValue>,
) -> Result<(), SettingsError> {
    let mut parts: Vec<&str> = key.split('.').collect();
    let Some(name) = parts.pop() else {
        return Ok(());
    };
    let mut table: &mut dyn TableLike = document.as_table_mut();
    let outer = parts.len().saturating_sub(1);
    for (at, part) in parts.into_iter().enumerate() {
        if value.is_none() && table.get(part).is_none() {
            return Ok(());
        }
        table = table
            .entry(part)
            .or_insert_with(|| section(at < outer))
            .as_table_like_mut()
            .ok_or(SettingsError::FileInvalid)?;
    }
    set(table, name, value);
    Ok(())
}

fn section(is_implicit: bool) -> Item {
    let mut table = toml_edit::Table::new();
    table.set_implicit(is_implicit);
    Item::Table(table)
}

fn set(table: &mut dyn TableLike, name: &str, value: Option<SettingValue>) {
    let Some(value) = value else {
        table.remove(name);
        return;
    };
    let mut value = Value::from(value);
    match table.get_mut(name) {
        Some(Item::Value(old)) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        Some(item) => *item = Item::Value(value),
        None => {
            table.insert(name, Item::Value(value));
        }
    }
}

#[must_use]
pub fn key_line(text: &str, key: &str) -> Option<usize> {
    let text = text.strip_prefix(BOM).unwrap_or(text);
    let document = toml_edit::Document::parse(text).ok()?;
    let mut parts: Vec<&str> = key.split('.').collect();
    let name = parts.pop()?;
    let mut table: &dyn TableLike = document.as_table();
    for part in parts {
        table = table.get(part)?.as_table_like()?;
    }
    let start = table.key(name)?.span()?.start;
    Some(text.get(..start)?.matches('\n').count() + 1)
}

impl super::Settings {
    #[must_use]
    pub fn value_at(&self, key: &str) -> Option<SettingValue> {
        let root = toml::Value::try_from(self).ok()?;
        let found = key
            .split('.')
            .try_fold(&root, |table, part| table.get(part))?;
        match found {
            toml::Value::Boolean(flag) => Some(SettingValue::Bool(*flag)),
            toml::Value::Integer(number) => Some(SettingValue::Integer(*number)),
            toml::Value::String(text) => Some(SettingValue::Text(text.clone())),
            toml::Value::Array(items) => items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .map(SettingValue::List),
            _ => None,
        }
    }
}

#[must_use]
pub fn section_line(text: &str, key: &str) -> usize {
    let plain = text.strip_prefix(BOM).unwrap_or(text);
    let last = plain.lines().count().max(1);
    let Ok(document) = toml_edit::Document::parse(plain) else {
        return last;
    };
    let mut parts: Vec<&str> = key.split('.').collect();
    parts.pop();
    let mut table: &dyn TableLike = document.as_table();
    let mut header = None;
    for part in parts {
        let Some(inner) = table.get(part).and_then(Item::as_table) else {
            break;
        };
        header = inner.span().map(|span| span.start).or(header);
        table = inner;
    }
    header
        .and_then(|start| plain.get(..start))
        .map_or(last, |before| before.matches('\n').count() + 1)
}
