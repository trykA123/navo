use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value, value};

use super::KeymapFileError;
use crate::text_layout::keep_layout;

const HEADER: &str =
    "# Your Navo shortcuts. Each [[bind]] replaces the built-in keys of one command.\n\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapEdit {
    Set {
        command: String,
        context: Option<String>,
        home: String,
        keys: Vec<String>,
    },
    Reset {
        command: String,
    },
}

pub fn patch_keymap(text: &str, edits: &[KeymapEdit]) -> Result<String, KeymapFileError> {
    keep_layout(text, |plain| patch_plain(plain, edits))
}

fn patch_plain(text: &str, edits: &[KeymapEdit]) -> Result<String, KeymapFileError> {
    let mut document: DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| KeymapFileError::FileInvalid(error.to_string()))?;
    let is_blank = text.trim().is_empty();
    for edit in edits {
        match edit {
            KeymapEdit::Set {
                command,
                context,
                home,
                keys,
            } => {
                let slot = Slot {
                    command,
                    context: context.as_deref(),
                    home,
                    is_blank,
                };
                set(&mut document, &slot, keys)?;
            }
            KeymapEdit::Reset { command } => reset(&mut document, command)?,
        }
    }
    Ok(document.to_string())
}

struct Slot<'a> {
    command: &'a str,
    context: Option<&'a str>,
    home: &'a str,
    is_blank: bool,
}

fn text_of<'a>(table: &'a Table, name: &str) -> Option<&'a str> {
    table.get(name).and_then(Item::as_str)
}

fn is_slot(table: &Table, slot: &Slot) -> bool {
    let context = text_of(table, "context").unwrap_or(slot.home);
    let context = context.replace("FerritePane", "NavoPane");
    text_of(table, "command") == Some(slot.command) && context == slot.context.unwrap_or(slot.home)
}

fn key_list(keys: &[String]) -> Value {
    let mut list: Array = keys.iter().map(String::as_str).collect();
    list.fmt();
    Value::Array(list)
}

fn set(document: &mut DocumentMut, slot: &Slot, keys: &[String]) -> Result<(), KeymapFileError> {
    let binds = document
        .entry("bind")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(KeymapFileError::Layout)?;
    if let Some(table) = binds.iter_mut().find(|table| is_slot(table, slot)) {
        replace_keys(table, keys);
        return Ok(());
    }
    let mut table = Table::new();
    table.insert("command", value(slot.command));
    table.insert("keys", Item::Value(key_list(keys)));
    if let Some(context) = slot.context {
        table.insert("context", value(context));
    }
    let gap = if slot.is_blank && binds.is_empty() {
        HEADER
    } else {
        "\n"
    };
    table.decor_mut().set_prefix(gap);
    binds.push(table);
    Ok(())
}

fn replace_keys(table: &mut Table, keys: &[String]) {
    let mut list = key_list(keys);
    match table.get_mut("keys") {
        Some(Item::Value(old)) => {
            *list.decor_mut() = old.decor().clone();
            *old = list;
        }
        _ => {
            table.insert("keys", Item::Value(list));
        }
    }
}

fn reset(document: &mut DocumentMut, command: &str) -> Result<(), KeymapFileError> {
    let Some(item) = document.get_mut("bind") else {
        return Ok(());
    };
    let binds = item
        .as_array_of_tables_mut()
        .ok_or(KeymapFileError::Layout)?;
    let carried = remove_command(binds, command);
    if !carried.is_empty() {
        let trailing = document.trailing().as_str().unwrap_or_default();
        let joined = format!("{carried}{trailing}");
        document.set_trailing(joined);
    }
    Ok(())
}

fn prefix_of(table: &Table) -> &str {
    table
        .decor()
        .prefix()
        .and_then(|raw| raw.as_str())
        .unwrap_or_default()
}

fn remove_command(binds: &mut ArrayOfTables, command: &str) -> String {
    let mut carried = String::new();
    let mut index = 0;
    while let Some(table) = binds.get_mut(index) {
        if text_of(table, "command") == Some(command) {
            carried.push_str(&comments_of(table));
            binds.remove(index);
            continue;
        }
        if !carried.is_empty() {
            let joined = format!("{carried}{}", prefix_of(table));
            table.decor_mut().set_prefix(joined);
            carried.clear();
        }
        index += 1;
    }
    carried
}

fn comments_of(table: &Table) -> String {
    let prefix = prefix_of(table);
    if !prefix.contains('#') {
        return String::new();
    }
    let kept = prefix.trim_end_matches(['\n', ' ', '\t']);
    format!("{kept}\n")
}
