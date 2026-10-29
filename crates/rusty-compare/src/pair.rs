use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use rusty_core::{ProviderPath, WindowsFoldKey};

use crate::model::{
    CaseRule, ContentClass, EntryKind, Generation, RelPath, Row, SideEntry, Status,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum GroupKey {
    Exact(RelPath),
    Folded(WindowsFoldKey),
}

pub fn pair(left: Vec<SideEntry>, right: Vec<SideEntry>, case: CaseRule) -> Vec<Row> {
    let mut rows = Vec::with_capacity(left.len().max(right.len()));
    let mut left = group(left, case);
    for (key, right_entries) in group(right, case) {
        let left_entries = left.remove(&key).unwrap_or_default();
        rows.extend(pair_group(left_entries, right_entries, case));
    }
    for left_entries in left.into_values() {
        rows.extend(pair_group(left_entries, Vec::new(), case));
    }
    rows.sort_by(compare_rows);
    rows
}

fn group(entries: Vec<SideEntry>, case: CaseRule) -> HashMap<GroupKey, Vec<SideEntry>> {
    let mut groups: HashMap<GroupKey, Vec<SideEntry>> = HashMap::new();
    for entry in entries {
        let key = match case {
            CaseRule::Sensitive => GroupKey::Exact(entry.rel.clone()),
            CaseRule::Insensitive => GroupKey::Folded(entry.rel.as_path().windows_fold_key()),
        };
        groups.entry(key).or_default().push(entry);
    }
    groups
}

fn pair_group(left: Vec<SideEntry>, right: Vec<SideEntry>, case: CaseRule) -> Vec<Row> {
    let conflicted = case == CaseRule::Insensitive && (left.len() > 1 || right.len() > 1);
    if conflicted {
        return case_conflict_rows(left, right);
    }
    let mut rows = Vec::with_capacity(left.len().max(right.len()));
    let mut right = right.into_iter();
    for left_entry in left {
        match right.next() {
            Some(right_entry) => rows.push(paired_row(left_entry, right_entry)),
            None => rows.push(single_row(left_entry, Status::OnlyLeft)),
        }
    }
    for right_entry in right {
        rows.push(single_row(right_entry, Status::OnlyRight));
    }
    rows
}

fn case_conflict_rows(left: Vec<SideEntry>, right: Vec<SideEntry>) -> Vec<Row> {
    let mut right_entries: Vec<Option<SideEntry>> = right.into_iter().map(Some).collect();
    let mut by_path: HashMap<RelPath, VecDeque<usize>> = HashMap::new();
    for (index, entry) in right_entries.iter().enumerate() {
        if let Some(entry) = entry {
            by_path
                .entry(entry.rel.clone())
                .or_default()
                .push_back(index);
        }
    }
    let mut pending = Vec::new();
    let mut rows = Vec::new();
    for left_entry in left {
        let matched = by_path
            .get_mut(&left_entry.rel)
            .and_then(VecDeque::pop_front)
            .and_then(|index| right_entries.get_mut(index).and_then(Option::take));
        match matched {
            Some(right_entry) => {
                rows.push(row(
                    Some(left_entry),
                    Some(right_entry),
                    Status::CaseConflict,
                ));
            }
            None => pending.push(left_entry),
        }
    }
    let remaining = right_entries.into_iter().flatten().collect();
    rows.extend(pair_in_order(pending, remaining, Status::CaseConflict));
    rows
}

fn pair_in_order(left: Vec<SideEntry>, right: Vec<SideEntry>, status: Status) -> Vec<Row> {
    let mut rows = Vec::with_capacity(left.len().max(right.len()));
    let mut right = right.into_iter();
    for left_entry in left {
        match right.next() {
            Some(right_entry) => rows.push(row(Some(left_entry), Some(right_entry), status)),
            None => rows.push(row(Some(left_entry), None, status)),
        }
    }
    for right_entry in right {
        rows.push(row(None, Some(right_entry), status));
    }
    rows
}

fn paired_row(left: SideEntry, right: SideEntry) -> Row {
    let status = if left.kind == right.kind {
        Status::Checking
    } else {
        Status::TypeConflict
    };
    row(Some(left), Some(right), status)
}

fn single_row(entry: SideEntry, status: Status) -> Row {
    let (left, right) = match status {
        Status::OnlyRight => (None, Some(entry)),
        _ => (Some(entry), None),
    };
    row(left, right, status)
}

fn row(left: Option<SideEntry>, right: Option<SideEntry>, status: Status) -> Row {
    Row {
        left: left.map(Arc::new),
        right: right.map(Arc::new),
        raw_status: status,
        display_status: status,
        generation: Generation::default(),
        content: ContentClass::Unknown,
    }
}

fn compare_rows(a: &Row, b: &Row) -> Ordering {
    folder_rank(a)
        .cmp(&folder_rank(b))
        .then_with(|| compare_entries(primary(a), primary(b)))
        .then_with(|| compare_entries(a.left.as_deref(), b.left.as_deref()))
        .then_with(|| compare_entries(a.right.as_deref(), b.right.as_deref()))
}

fn folder_rank(row: &Row) -> u8 {
    let is_folder = row
        .left
        .as_deref()
        .is_some_and(|entry| entry.kind == EntryKind::Folder)
        || row
            .right
            .as_deref()
            .is_some_and(|entry| entry.kind == EntryKind::Folder);
    u8::from(!is_folder)
}

fn primary(row: &Row) -> Option<&SideEntry> {
    row.left.as_deref().or(row.right.as_deref())
}

fn compare_entries(a: Option<&SideEntry>, b: Option<&SideEntry>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => compare_paths(a.rel.as_path(), b.rel.as_path())
            .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
            .then_with(|| a.size.cmp(&b.size))
            .then_with(|| a.modified.cmp(&b.modified))
            .then_with(|| a.ignored.cmp(&b.ignored)),
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
    }
}

fn compare_paths(a: &ProviderPath, b: &ProviderPath) -> Ordering {
    compare_canonical(a, b).then_with(|| compare_raw(a, b))
}

fn compare_canonical(a: &ProviderPath, b: &ProviderPath) -> Ordering {
    match (a.as_str(), b.as_str()) {
        (Some(a), Some(b)) => return a.cmp(b),
        (Some(text), None) => return compare_text_to_units(text, b),
        (None, Some(text)) => return compare_text_to_units(text, a).reverse(),
        (None, None) => {}
    }
    match (a.to_wide(), b.to_wide()) {
        (Some(a), Some(b)) => canonical_units(&a).cmp(canonical_units(&b)),
        (None, None) => match (a.to_bytes(), b.to_bytes()) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => Ordering::Equal,
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
    }
}

fn compare_raw(a: &ProviderPath, b: &ProviderPath) -> Ordering {
    match (a.to_wide(), b.to_wide()) {
        (Some(a), Some(b)) => a.cmp(&b),
        (None, None) => match (a.to_bytes(), b.to_bytes()) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => Ordering::Equal,
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
    }
}

fn compare_text_to_units(text: &str, path: &ProviderPath) -> Ordering {
    match path.to_wide() {
        Some(units) => text.chars().cmp(canonical_units(&units)),
        None => Ordering::Less,
    }
}

fn canonical_units(units: &[u16]) -> impl Iterator<Item = char> + '_ {
    char::decode_utf16(units.iter().copied()).map(|decoded| match decoded {
        Ok(character) => character,
        Err(error) => wide_character(error.unpaired_surrogate()),
    })
}

fn wide_character(unit: u16) -> char {
    char::from_u32(u32::from(unit) - 0xD800 + 0x10_0000).unwrap_or(char::REPLACEMENT_CHARACTER)
}

const fn kind_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::File => 0,
        EntryKind::Folder => 1,
        EntryKind::Link => 2,
        EntryKind::Submodule => 3,
    }
}

#[cfg(test)]
mod tests;
