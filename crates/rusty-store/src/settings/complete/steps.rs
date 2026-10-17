use toml_edit::{DocumentMut, Item, Table, TableLike};

use super::super::{Settings, SettingsError};
use super::plain::{Source, edit};
use super::tree::{Entry, bare, entries, lookup};
use super::{DefaultChange, Ledger, Reconciled};

pub fn complete_text(text: &str, defaults: &Settings) -> Result<Reconciled, SettingsError> {
    let entries = entries(defaults);
    let (text, written) = edit(text, |document, source| {
        let missing: Vec<&Entry> = entries
            .iter()
            .filter(|entry| lookup(&source.file, &entry.path).is_none())
            .collect();
        if !missing.is_empty() {
            keep_lone_comments_on_top(document);
        }
        missing
            .into_iter()
            .filter(|entry| insert(document, entry))
            .map(|entry| (entry.key.clone(), entry.canonical.clone()))
            .collect()
    })?;
    Ok(Reconciled {
        text,
        written,
        changes: Vec::new(),
    })
}

pub fn upgrade_text(
    text: &str,
    ledger: &Ledger,
    defaults: &Settings,
) -> Result<Reconciled, SettingsError> {
    let entries = entries(defaults);
    let (text, (written, changes)) = edit(text, |document, source| {
        let mut written = Vec::new();
        let mut changes = Vec::new();
        for entry in &entries {
            match follow(entry, ledger, source) {
                Follow::Upgrade(old) if replace(document, entry) => {
                    written.push((entry.key.clone(), entry.canonical.clone()));
                    changes.push(DefaultChange {
                        key: entry.key.clone(),
                        old,
                        new: entry.canonical.clone(),
                    });
                }
                Follow::Adopt => written.push((entry.key.clone(), entry.canonical.clone())),
                Follow::Upgrade(_) | Follow::Leave => {}
            }
        }
        (written, changes)
    })?;
    Ok(Reconciled {
        text,
        written,
        changes,
    })
}

enum Follow {
    Upgrade(String),
    Adopt,
    Leave,
}

fn follow(entry: &Entry, ledger: &Ledger, source: &Source) -> Follow {
    let Some(current) = lookup(&source.file, &entry.path).map(ToString::to_string) else {
        return Follow::Leave;
    };
    match ledger.get(&entry.key) {
        Some(recorded) if *recorded == current && current != entry.canonical => {
            Follow::Upgrade(current)
        }
        None if current == entry.canonical => Follow::Adopt,
        _ => Follow::Leave,
    }
}

fn keep_lone_comments_on_top(document: &mut DocumentMut) {
    let mut trailing = document.trailing().as_str().unwrap_or_default().to_owned();
    if document.as_table().is_empty() && !trailing.is_empty() {
        if !trailing.ends_with('\n') {
            trailing.push('\n');
        }
        document.decor_mut().set_prefix(trailing);
        document.set_trailing("");
    }
}

fn insert(document: &mut DocumentMut, entry: &Entry) -> bool {
    let Some((name, parents)) = entry.path.split_last() else {
        return false;
    };
    let is_first = document.as_table().is_empty();
    let mut table: &mut dyn TableLike = document.as_table_mut();
    for part in parents {
        let section = table.entry(part).or_insert_with(|| new_section(is_first));
        let Some(next) = section.as_table_like_mut() else {
            return false;
        };
        table = next;
    }
    table.insert(name, Item::Value(bare(&entry.value)));
    true
}

fn new_section(is_first: bool) -> Item {
    let mut section = Table::new();
    if !is_first {
        section.decor_mut().set_prefix("\n");
    }
    Item::Table(section)
}

fn replace(document: &mut DocumentMut, entry: &Entry) -> bool {
    let Some((name, parents)) = entry.path.split_last() else {
        return false;
    };
    let mut table: &mut dyn TableLike = document.as_table_mut();
    for part in parents {
        let Some(next) = table.get_mut(part).and_then(Item::as_table_like_mut) else {
            return false;
        };
        table = next;
    }
    let Some(Item::Value(old)) = table.get_mut(name) else {
        return false;
    };
    let mut value = bare(&entry.value);
    *value.decor_mut() = old.decor().clone();
    *old = value;
    true
}
