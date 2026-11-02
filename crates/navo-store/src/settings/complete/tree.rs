use std::collections::BTreeMap;

use toml_edit::{DocumentMut, Item, Table, Value};

use super::super::{Settings, SettingsError, to_toml};
use super::plain::without_bom;

pub(super) struct Entry {
    pub path: Vec<String>,
    pub key: String,
    pub value: Value,
    pub canonical: String,
}

pub(super) fn entries(defaults: &Settings) -> Vec<Entry> {
    let document: DocumentMut = to_toml(defaults).parse().unwrap_or_default();
    let mut found = Vec::new();
    collect(document.as_table(), &mut Vec::new(), &mut found);
    found
        .into_iter()
        .filter_map(|(path, value)| {
            let canonical = canonical(&value)?;
            Some(Entry {
                key: path.join("."),
                path,
                value,
                canonical,
            })
        })
        .collect()
}

fn collect(table: &Table, path: &mut Vec<String>, found: &mut Vec<(Vec<String>, Value)>) {
    for (key, item) in table {
        path.push(key.to_owned());
        match item {
            Item::Value(value) => found.push((path.clone(), bare(value))),
            Item::Table(inner) => collect(inner, path, found),
            Item::None | Item::ArrayOfTables(_) => {}
        }
        path.pop();
    }
}

pub(super) fn bare(value: &Value) -> Value {
    let mut value = value.clone();
    value.decor_mut().clear();
    value
}

fn canonical(value: &Value) -> Option<String> {
    let mut table: toml::Table = toml::from_str(&format!("v = {value}")).ok()?;
    table.remove("v").map(|value| value.to_string())
}

pub(super) fn lookup<'a>(table: &'a toml::Table, path: &[String]) -> Option<&'a toml::Value> {
    let (name, parents) = path.split_last()?;
    let parent = parents
        .iter()
        .try_fold(table, |table, part| table.get(part)?.as_table())?;
    parent.get(name)
}

pub(super) fn leaves(text: &str) -> Result<BTreeMap<Vec<String>, String>, SettingsError> {
    let table: toml::Table =
        toml::from_str(&without_bom(text)).map_err(|_| SettingsError::FileInvalid)?;
    let mut found = BTreeMap::new();
    flatten(&table, &mut Vec::new(), &mut found);
    Ok(found)
}

fn flatten(table: &toml::Table, path: &mut Vec<String>, found: &mut BTreeMap<Vec<String>, String>) {
    for (key, value) in table {
        path.push(key.clone());
        match value {
            toml::Value::Table(inner) => flatten(inner, path, found),
            other => {
                found.insert(path.clone(), other.to_string());
            }
        }
        path.pop();
    }
}
