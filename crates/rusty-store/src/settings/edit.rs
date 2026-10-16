use toml_edit::{DocumentMut, Item, TableLike, Value};

use super::SettingsError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Text(String),
}

impl From<SettingValue> for Value {
    fn from(value: SettingValue) -> Self {
        match value {
            SettingValue::Bool(flag) => flag.into(),
            SettingValue::Integer(number) => number.into(),
            SettingValue::Text(text) => text.into(),
        }
    }
}

pub(super) fn patch(
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
    for part in parts {
        if value.is_none() && table.get(part).is_none() {
            return Ok(text.to_owned());
        }
        table = table
            .entry(part)
            .or_insert_with(toml_edit::table)
            .as_table_like_mut()
            .ok_or(SettingsError::FileInvalid)?;
    }
    set(table, name, value);
    Ok(document.to_string())
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
            _ => None,
        }
    }
}
