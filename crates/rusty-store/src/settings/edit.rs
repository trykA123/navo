use toml_edit::{DocumentMut, Item, TableLike, Value};

use super::SettingsError;
use crate::text_layout::{BOM, keep_layout};

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

pub(super) fn patch(
    text: &str,
    key: &str,
    value: Option<SettingValue>,
) -> Result<String, SettingsError> {
    keep_layout(text, |plain| patch_plain(plain, key, value))
}

fn patch_plain(
    text: &str,
    key: &str,
    value: Option<SettingValue>,
) -> Result<String, SettingsError> {
    let mut document: DocumentMut = text.parse().map_err(|_| SettingsError::FileInvalid)?;
    let mut parts: Vec<&str> = key.split('.').collect();
    let Some(name) = parts.pop() else {
        return Ok(text.to_owned());
    };
    let mut table: &mut dyn TableLike = document.as_table_mut();
    let outer = parts.len().saturating_sub(1);
    for (at, part) in parts.into_iter().enumerate() {
        if value.is_none() && table.get(part).is_none() {
            return Ok(text.to_owned());
        }
        table = table
            .entry(part)
            .or_insert_with(|| section(at < outer))
            .as_table_like_mut()
            .ok_or(SettingsError::FileInvalid)?;
    }
    set(table, name, value);
    Ok(document.to_string())
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

pub(super) fn key_source(text: &str, key: &str) -> Result<Option<(String, String)>, SettingsError> {
    let plain = text.strip_prefix(BOM).unwrap_or(text);
    let document: DocumentMut = plain.parse().map_err(|_| SettingsError::FileInvalid)?;
    let item = key
        .split('.')
        .try_fold(document.as_item(), |item, name| item.get(name));
    Ok(item.map(|item| {
        let line = key_line(text, key)
            .and_then(|line| text.lines().nth(line - 1))
            .unwrap_or_default();
        (line.to_owned(), item.to_string())
    }))
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
