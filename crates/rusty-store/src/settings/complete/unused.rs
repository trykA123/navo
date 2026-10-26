use toml_edit::{Decor, DocumentMut, Item, Table};

use super::super::SettingsError;
use super::plain::edit;

pub const UNUSED_NOTE: &str = "# no longer used by Navo";

pub fn mark_unused(text: &str) -> Result<String, SettingsError> {
    let (text, ()) = edit(text, |document, source| {
        for path in &source.ignored {
            note(document, path);
        }
    })?;
    Ok(text)
}

fn note(document: &mut DocumentMut, path: &[String]) {
    let Some((name, parents)) = path.split_last() else {
        return;
    };
    let mut table = document.as_table_mut();
    for part in parents {
        let Some(next) = table.get_mut(part).and_then(Item::as_table_mut) else {
            return;
        };
        table = next;
    }
    match table.get_mut(name) {
        Some(Item::Table(section)) => note_section(section),
        Some(Item::Value(_)) => {
            if let Some(mut key) = table.key_mut(name) {
                add_note(key.leaf_decor_mut());
            }
        }
        _ => {}
    }
}

fn note_section(section: &mut Table) {
    if section.is_dotted() {
        return;
    }
    if !section.is_implicit() {
        add_note(section.decor_mut());
        return;
    }
    for (_, item) in section.iter_mut() {
        if let Item::Table(child) = item {
            note_section(child);
        }
    }
}

fn add_note(decor: &mut Decor) {
    let prefix = decor
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .unwrap_or_default()
        .to_owned();
    let (head, indent) = prefix
        .rfind('\n')
        .map_or(("", prefix.as_str()), |at| prefix.split_at(at + 1));
    if head.lines().last().map(str::trim) == Some(UNUSED_NOTE) {
        return;
    }
    decor.set_prefix(format!("{head}{indent}{UNUSED_NOTE}\n{indent}"));
}
